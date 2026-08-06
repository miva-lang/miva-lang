use std::collections::{HashMap, HashSet};
use std::fs::exists;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;

use super::{color, dependency_graph::DependencyGraph, env, frontend, lock};
use crate::ast::{Def, Param, Safety, Typ};
use crate::codegen;
use crate::codegen::Backend;
use crate::commands::clean;
use crate::config::Config;
use crate::error;
use crate::macro_expand;
use crate::magical;
use crate::semantic;
use crate::typecheck;
use crate::warning;

mod cache;
mod compile;
mod host;
mod imports;
mod sigs;

pub(crate) use cache::*;
pub(crate) use compile::*;
pub(crate) use host::*;
pub(crate) use imports::*;
pub(crate) use sigs::*;

#[derive(clap::Args)]
pub struct Args {
    #[arg(
        short = 'b',
        long,
        help = "Backend to use: cxx (default) or llvm. Overrides miva.toml project.backend."
    )]
    pub backend: Option<String>,
}

fn find_project_root() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        if current.join("miva.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn determine_entry(project_type: &str) -> &str {
    if project_type == "lib" {
        "src/lib.miva"
    } else {
        "src/main.miva"
    }
}

pub fn exec(verbose: bool, release: bool, cli_backend: Option<String>) -> Result<()> {
    let project_root = find_project_root()
        .ok_or_else(|| anyhow::anyhow!("no miva.toml found. Run `miva init <name>` first."))?;

    std::env::set_current_dir(&project_root)?;

    let config = Config::load().ok_or_else(|| anyhow::anyhow!("failed to parse miva.toml"))?;

    let project = config
        .project
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing [project] section in miva.toml"))?;

    let name = project
        .name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing project.name in miva.toml"))?;

    let project_type = project
        .project_type
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing project.type in miva.toml"))?;

    let backend_from_config = config
        .project_backend()
        .unwrap_or_else(|| "cxx".to_string());
    let backend_name = cli_backend.unwrap_or(backend_from_config);
    let backend = Backend::from_name(&backend_name).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown backend '{}'. Use 'cxx', 'llvm', or 'mvm'.",
            backend_name
        )
    })?;

    let entry_file = determine_entry(project_type);

    if !Path::new(entry_file).exists() {
        anyhow::bail!("entry file not found: {}", entry_file);
    }

    let std_include_dir = env::get_std_include_dir();
    let cache_dir = env::get_cache_dir_rel(release);
    let build_dir = env::get_build_dir_rel(release);
    std::fs::create_dir_all(&cache_dir)?;
    std::fs::create_dir_all(&build_dir)?;

    eprintln!(
        "{}",
        color::info(&format!(
            "building {} ({}) [backend: {}]",
            name,
            project_type,
            backend.name()
        ))
    );

    let std_path_str = std_include_dir.to_string_lossy();

    // Resolve dependencies
    let declared = config.dependencies();
    let deps = if declared.is_empty() {
        HashMap::new()
    } else {
        lock::resolve(&declared, &std_include_dir)?
    };

    if !deps.is_empty() {
        eprintln!(
            "  {}",
            color::info(&format!(
                "dependencies: {}",
                deps.iter()
                    .map(|(n, v)| format!("{}={}", n, v))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        );
    }

    // Build dependency graph while collecting source files
    let mut ast_cache: AstCache = HashMap::new();
    let mut visited = HashSet::new();
    let mut graph = DependencyGraph::new();
    collect_imports_with_graph(
        &mut ast_cache,
        entry_file,
        &mut visited,
        &cache_dir,
        &std_path_str,
        &mut graph,
        &name,
        &deps,
    )?;

    let mut files: Vec<String> = Vec::new();
    let std_str = if let Some(std_ver) = deps.get("std") {
        let ver_dir = format!("std-{}", std_ver);
        std_include_dir.join(ver_dir).join("src/str.miva")
    } else {
        std_include_dir.join("std/src/str.miva")
    };
    if std_str.exists() {
        files.push(std_str.to_string_lossy().to_string());
    }
    files.push(entry_file.to_string());
    for file in &visited {
        if !files.contains(file) {
            files.push(file.clone());
        }
    }

    // Phase 0: Collect macro definitions from all files (cross-file availability)
    let macro_table = collect_all_macros(&mut ast_cache, &files);
    let macro_count = macro_table.len();
    if macro_count > 0 {
        eprintln!(
            "{}",
            color::step(
                "macros",
                &format!("{} custom macro(s) collected", macro_count)
            )
        );
    }

    // Phase 0.5: Collect function signatures from all files (cross-file type info)
    let GlobalSigs {
        all_func_sigs,
        global_type_sigs,
        global_safety,
        global_enums,
    } = collect_global_sigs(&mut ast_cache, &files, &macro_table, &name)?;

    // Phase 1: Compile each .miva to source file (content-hash based caching)
    let mut src_results: Vec<(String, PathBuf, bool)> = Vec::new();
    let mut recompiled_files: HashSet<String> = HashSet::new();

    for file in &files {
        eprintln!("{}", color::step("compile", file));
        let cache_key = make_cache_key(file, &std_path_str);
        let was_cached = !needs_rebuild_by_hash(file, &cache_dir, &cache_key, backend);

        let (src_path, has_main) = compile_file_to_src(
            &mut ast_cache,
            file,
            &cache_dir,
            &std_path_str,
            verbose,
            &macro_table,
            backend,
            &all_func_sigs,
            &global_type_sigs,
            &global_safety,
            &global_enums,
        )?;

        if !was_cached {
            recompiled_files.insert(file.clone());
        }

        src_results.push((file.clone(), src_path, has_main));
    }

    // Determine which files need source -> .o compilation
    let mut need_compile_bin: HashSet<String> = HashSet::new();
    for file in &recompiled_files {
        need_compile_bin.insert(file.clone());
        for dependent in graph.get_all_dependents(file) {
            need_compile_bin.insert(dependent);
        }
    }

    let all_cached = need_compile_bin.is_empty();

    // Create cache symlinks for versioned dep includes
    for (dep_name, dep_ver) in &deps {
        let versioned_cache = cache_dir.join(format!("{}-{}", dep_name, dep_ver));
        let unversioned_link = cache_dir.join(dep_name);
        if versioned_cache.exists() && !unversioned_link.exists() {
            std::os::unix::fs::symlink(
                versioned_cache
                    .strip_prefix(&cache_dir)
                    .unwrap_or(&versioned_cache),
                &unversioned_link,
            )?;
        }
    }

    // MVM backend: compile all defs to a single .mvm file
    if backend == Backend::Mvm {
        let mvm_path = if project_type == "lib" {
            build_dir.join(format!("lib{}.mvm", name))
        } else {
            build_dir.join(format!("{}.mvm", name))
        };

        // Check if bytecode needs rebuild (if any source changed)
        let needs_build = if mvm_path.exists() {
            let all_files_up_to_date = files.iter().all(|f| {
                let cache_key = make_cache_key(f, &std_path_str);
                !needs_rebuild_by_hash(f, &cache_dir, &cache_key, backend)
            });
            !all_files_up_to_date
        } else {
            true
        };

        if needs_build {
            eprintln!("{}", color::step("compile", "mvm bytecode"));

            // Collect all expanded defs from all files
            let mut all_defs: Vec<Def> = Vec::new();
            for file in &files {
                let ast = parse_cached(&mut ast_cache, file)?;
                let defs = macro_expand::expand_macros(&ast.defs, &macro_table)?;
                all_defs.extend(defs);
            }

            // Annotate lambda captures so the MVM backend can lower closures.
            typecheck::annotate_lambda_captures(&mut all_defs);
            crate::drop_desugar::desugar_drops(&mut all_defs);

            // Generate combined MVM bytecode
            let output = codegen::build_ir_with_backend(&all_defs, backend, &all_func_sigs);

            // Write .mvm file
            std::fs::create_dir_all(&build_dir)?;
            std::fs::write(&mvm_path, &output.program)?;

            // Generate and compile the project's single libhost.so for any
            // user `unsafe fn` (raw C) definitions.
            if !output.host_defs.is_empty() {
                compile_libhost(&build_dir, &output.host_defs, HostKind::SharedLib)?;
            } else if build_dir.join("libhost.so").exists() {
                let _ = std::fs::remove_file(build_dir.join("libhost.so"));
            }

            // Update hash cache for all files
            for file in &files {
                let cache_key = make_cache_key(file, &std_path_str);
                update_hash_cache(file, &cache_dir, &cache_key, backend);
            }

            eprintln!(
                "{}",
                color::success(&format!("{} -> {}", name, mvm_path.display()))
            );
        }

        println!("{}", color::success("compilation finished"));
        return Ok(());
    }

    // LLVM backend: compile user inline-unsafe C functions into libhost.o and
    // link it (same raw C shims as the MVM backend's libhost.so, but a static
    // object linked directly into the executable).
    let mut llvm_host_obj: Option<PathBuf> = None;
    if backend == Backend::Llvm {
        let mut all_defs: Vec<Def> = Vec::new();
        for file in &files {
            let ast = parse_cached(&mut ast_cache, file)?;
            let defs = macro_expand::expand_macros(&ast.defs, &macro_table)?;
            all_defs.extend(defs);
        }
        typecheck::annotate_lambda_captures(&mut all_defs);
        crate::drop_desugar::desugar_drops(&mut all_defs);
        let output = codegen::build_ir_with_backend(&all_defs, backend, &all_func_sigs);
        if !output.host_defs.is_empty() {
            let libhost_o = compile_libhost(&build_dir, &output.host_defs, HostKind::Object)?;
            llvm_host_obj = Some(libhost_o);
        }
    }

    // Phase 2: Compile source to .o
    let mut obj_files: Vec<PathBuf> = Vec::new();
    let mut dep_include_dirs: Vec<PathBuf> = Vec::new();
    for (dep_name, dep_ver) in &deps {
        dep_include_dirs.push(std_include_dir.join(format!("{}-{}", dep_name, dep_ver)));
    }

    let mut bridge_compiled = false;
    for (_i, (file, src_path, _)) in src_results.iter().enumerate() {
        if all_cached {
            let obj_path = src_path.with_extension("o");
            if obj_path.exists() {
                obj_files.push(obj_path);
                continue;
            }
        } else if !need_compile_bin.contains(file) {
            let obj_path = src_path.with_extension("o");
            if obj_path.exists() {
                obj_files.push(obj_path);
                continue;
            }
        }

        let obj_path = compile_src_to_obj(
            src_path,
            &cache_dir,
            &std_include_dir,
            &build_dir,
            project_type,
            release,
            verbose,
            &dep_include_dirs,
            backend,
        )?;
        if backend == Backend::Llvm && !bridge_compiled {
            let bridge_src = src_path.with_extension("bridge.cpp");
            let bridge_obj = src_path.with_extension("bridge.o");
            if bridge_src.exists() {
                compile_bridge_obj(
                    &bridge_src,
                    &bridge_obj,
                    &cache_dir,
                    &std_include_dir,
                    &build_dir,
                    &dep_include_dirs,
                    release,
                    verbose,
                )?;
                obj_files.push(bridge_obj);
                bridge_compiled = true;
            }
        }

        obj_files.push(obj_path);
    }

    // Fallback: compile bridge if skipped due to caching (LLVM backend)
    if backend == Backend::Llvm && !bridge_compiled {
        for (_i, (_file, src_path, _)) in src_results.iter().enumerate() {
            let bridge_src = src_path.with_extension("bridge.cpp");
            let bridge_obj = src_path.with_extension("bridge.o");
            if bridge_src.exists() {
                if !bridge_obj.exists() {
                    compile_bridge_obj(
                        &bridge_src,
                        &bridge_obj,
                        &cache_dir,
                        &std_include_dir,
                        &build_dir,
                        &dep_include_dirs,
                        release,
                        verbose,
                    )?;
                }
                obj_files.push(bridge_obj);
                bridge_compiled = true;
                break;
            }
        }
    }

    obj_files.sort();
    obj_files.dedup();

    if let Some(host_obj) = &llvm_host_obj {
        obj_files.push(host_obj.clone());
    }

    eprintln!("{}", color::step("link", name));

    let _ = link_objects(&obj_files, name, &build_dir, project_type, release)?;

    if !env::get_keep_cpp() {
        for obj in &obj_files {
            let src = obj.with_extension(backend.extension());
            let _ = std::fs::remove_file(src);
        }
    }

    if project_type == "lib" {
        let lib_header = cache_dir.join("src/lib.h");
        if lib_header.exists() {
            let dest = build_dir.join(format!("{}.h", name));
            std::fs::copy(&lib_header, &dest)?;
        }
    }

    println!("{}", color::success("compilation finished"),);

    Ok(())
}
