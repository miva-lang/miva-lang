use super::*;

pub(crate) fn compile_file_to_src(
    ast_cache: &mut AstCache,
    file: &str,
    cache_dir: &Path,
    std_path: &str,
    _verbose: bool,
    macro_table: &macro_expand::MacroTable,
    backend: Backend,
    func_sigs: &std::collections::HashMap<String, crate::codegen::FuncSig>,
    global_type_sigs: &std::collections::HashMap<String, (Vec<String>, Vec<Param>, Option<Typ>)>,
    global_safety: &std::collections::HashMap<String, Safety>,
    global_enums: &std::collections::HashMap<String, Vec<crate::ast::EnumVariant>>,
) -> Result<(PathBuf, bool)> {
    let cache_key = make_cache_key(file, std_path);
    let ext = backend.extension();
    let src_path = cache_dir.join(format!("{}.{}", cache_key, ext));

    if src_path.exists() && !needs_rebuild_by_hash(file, cache_dir, &cache_key, backend) {
        let has_main = std::fs::read_to_string(&src_path)
            .map(|s| s.contains("mvp_own_main"))
            .unwrap_or(false);
        return Ok((src_path, has_main));
    }

    let ast = parse_cached(ast_cache, file)?;

    let mut defs = macro_expand::expand_macros(&ast.defs, macro_table)?;

    let source = std::fs::read_to_string(file).unwrap_or_default();

    let sem_errors = semantic::check_program_with(&defs, global_safety, global_enums);
    if !sem_errors.is_empty() {
        for err in &sem_errors {
            eprintln!(
                "{}",
                color::colorize(
                    color::RED,
                    error::format_error_with_source(err, file, &source).as_str()
                )
            );
        }
        anyhow::bail!("semantic errors found");
    }

    let type_errors = typecheck::check_program_with(
        &defs,
        global_type_sigs,
        global_enums,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    );
    if !type_errors.is_empty() {
        for err in &type_errors {
            eprintln!(
                "{}",
                color::colorize(
                    color::RED,
                    error::format_error_with_source(err, file, &source).as_str()
                )
            );
        }
        anyhow::bail!("type errors found");
    }

    typecheck::annotate_lambda_captures(&mut defs);
    crate::drop_desugar::desugar_drops(&mut defs);

    let magical_flags = magical::get_magical_flags(&defs);
    let warnings = warning::get_warnings(&defs);
    let (warnings, err_warnings) = magical::filter_warnings(warnings, &magical_flags);
    for w in &err_warnings {
        eprintln!(
            "{}",
            color::colorize(
                color::RED,
                warning::format_warning_with_source(w, file, &source).as_str()
            )
        );
    }
    if !err_warnings.is_empty() {
        anyhow::bail!("some warnings treated as errors");
    }
    for w in &warnings {
        eprintln!(
            "{}",
            color::colorize(
                color::YELLOW,
                warning::format_warning_with_source(w, file, &source).as_str()
            )
        );
    }

    // For MVM backend: collect defs, skip per-file code generation.
    // Combined compilation happens later in exec(). Do NOT update hash
    // cache here — that would trick needs_build into thinking the file
    // is up-to-date when bytecode was never generated.
    if backend == Backend::Mvm {
        return Ok((src_path, false));
    }

    let output = codegen::build_ir_with_backend(&defs, backend, func_sigs);

    std::fs::create_dir_all(
        src_path.parent().ok_or_else(|| {
            anyhow::anyhow!("cannot determine parent directory of {:?}", src_path)
        })?,
    )?;
    std::fs::write(&src_path, &output.program)?;

    if !output.header.is_empty() {
        let header_path = cache_dir.join(format!("{}.h", cache_key));
        std::fs::write(&header_path, &output.header)?;
        // For LLVM backend, also write bridge file
        if backend == Backend::Llvm {
            let bridge_path = cache_dir.join(format!("{}.bridge.cpp", cache_key));
            std::fs::write(&bridge_path, &output.header)?;
        }
    }

    if !output.test.is_empty() {
        let test_ext = backend.extension();
        let test_path = cache_dir.join(format!("{}.test.{}", cache_key, test_ext));
        let test = if !output.header.is_empty() && backend == Backend::Cxx {
            let basename = std::path::Path::new(&cache_key)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&cache_key);
            format!("#include \"{}.h\"\n{}", basename, output.test)
        } else {
            output.test
        };
        std::fs::write(&test_path, &test)?;
    }

    update_hash_cache(file, cache_dir, &cache_key, backend);

    let prog_str = String::from_utf8_lossy(&output.program);
    let has_main = prog_str.contains("mvp_own_main");
    Ok((src_path, has_main))
}

pub(crate) fn path_to_str(p: &Path) -> Result<&str> {
    p.to_str()
        .ok_or_else(|| anyhow::anyhow!("path is not valid UTF-8: {:?}", p))
}

pub(crate) fn include_flag(dir: &[&Path]) -> Result<Vec<String>> {
    let mut flag = Vec::new();
    for i in dir {
        let f = format!("-I{}", path_to_str(i)?);
        flag.push(f);
    }
    Ok(flag)
}

/// The build directory only ever holds the output binary and generated headers
/// the *project itself* consumes. Exposing it as `-I` lets an `#include <tuple>`
/// resolve to a stray `build/debug/tuple` executable (same name as the C++
/// standard header), making g++ parse a binary and hang for minutes. `-iquote`
/// limits it to `#include "..."` forms so system `<...>` headers never collide.
pub(crate) fn include_build_dir(build_dir: &Path) -> Result<Vec<String>> {
    Ok(vec![format!("-iquote{}", path_to_str(build_dir)?)])
}

pub(crate) fn compile_src_to_obj(
    src_path: &Path,
    cache_dir: &Path,
    std_include: &Path,
    build_dir: &Path,
    project_type: &str,
    release: bool,
    verbose: bool,
    extra_includes: &[PathBuf],
    backend: Backend,
) -> Result<PathBuf> {
    let obj_path = src_path.with_extension("o");

    match backend {
        Backend::Cxx => {
            let opt_flag = if release { "-O2" } else { "-g" };
            let pic_flag = if project_type == "lib" { "-fPIC" } else { "" };
            let inc_flags = env::get_include_flags();
            let mut include = include_flag(&[cache_dir, std_include])?;
            include.extend(include_build_dir(build_dir)?);
            for extra in extra_includes {
                include.push(format!("-I{}", extra.to_string_lossy()));
            }

            let mut args = vec![
                opt_flag,
                "-std=c++20",
                "-c",
                path_to_str(src_path)?,
                "-o",
                path_to_str(&obj_path)?,
                "-Wno-template-body",
            ];

            if !pic_flag.is_empty() {
                args.push(pic_flag);
            }
            for flag in inc_flags.split_whitespace() {
                if !flag.is_empty() {
                    args.push(flag);
                }
            }
            for flag in &include {
                args.push(flag);
            }

            let mut cmd = Command::new("g++");
            cmd.args(&args);
            if !verbose {
                cmd.stderr(std::process::Stdio::null());
            }
            let compile_output = env::run_with_timeout(&mut cmd, "g++", true)
                .map_err(|e| anyhow::anyhow!("Failed to run g++: {}", e))?;

            if !compile_output.status.success() {
                let stderr = String::from_utf8_lossy(&compile_output.stderr);
                eprintln!("{}", color::error("g++ compilation failed"));
                if verbose {
                    eprintln!("{}", stderr);
                } else {
                    for line in stderr.lines().take(5) {
                        eprintln!("{}", line);
                    }
                }
                if !env::get_keep_cpp() {
                    clean::exec(false)?;
                }
                std::process::exit(1);
            }

            Ok(obj_path)
        }
        Backend::Llvm => {
            let mut cmd = Command::new("llc");
            cmd.args([
                "-filetype=obj",
                path_to_str(src_path)?,
                "-o",
                path_to_str(&obj_path)?,
            ]);
            let llc_output = env::run_with_timeout(&mut cmd, "llc", true)
                .map_err(|e| anyhow::anyhow!("Failed to run llc: {}", e))?;

            if !llc_output.status.success() {
                let stderr = String::from_utf8_lossy(&llc_output.stderr);
                eprintln!("{}", color::error("llc compilation failed"));
                if verbose {
                    eprintln!("{}", stderr);
                } else {
                    for line in stderr.lines().take(5) {
                        eprintln!("{}", line);
                    }
                }
                if !env::get_keep_cpp() {
                    clean::exec(false)?;
                }
                std::process::exit(1);
            }

            Ok(obj_path)
        }
        Backend::Mvm => {
            anyhow::bail!("compile_src_to_obj should not be called for MVM backend");
        }
    }
}

pub(crate) fn link_objects(
    obj_files: &[PathBuf],
    output_file: &str,
    build_dir: &Path,
    project_type: &str,
    _release: bool,
) -> Result<String> {
    let exe_name = if project_type == "lib" {
        format!("lib{}", output_file)
    } else {
        output_file.to_string()
    };

    let exe_path = build_dir.join(&exe_name);
    let final_path = if project_type == "lib" {
        exe_path.with_extension("so")
    } else {
        exe_path.clone()
    };

    let mut args = vec!["-O2", "-std=c++20", "-o", path_to_str(&final_path)?];

    for obj in obj_files {
        args.push(path_to_str(&obj)?);
    }

    if project_type == "lib" {
        args.push("-fPIC");
        args.push("-shared");
    }

    let link_flags = env::get_link_flags();
    for flag in link_flags.split_whitespace() {
        if !flag.is_empty() {
            args.push(flag);
        }
    }

    let mut cmd = Command::new("g++");
    cmd.args(&args);
    let output = env::run_with_timeout(&mut cmd, "g++ (linking)", true)
        .map_err(|e| anyhow::anyhow!("Failed to run g++ for linking: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("{}", color::error(&format!("linking failed:\n{}", stderr)));
        std::process::exit(1);
    }

    Ok(final_path.to_string_lossy().to_string())
}

/// Compile an LLVM-backend `.bridge.cpp` file to its `.bridge.o` object.
pub(crate) fn compile_bridge_obj(
    bridge_src: &Path,
    bridge_obj: &Path,
    cache_dir: &Path,
    std_include_dir: &Path,
    build_dir: &Path,
    dep_include_dirs: &[PathBuf],
    release: bool,
    verbose: bool,
) -> Result<()> {
    let opt_flag = if release { "-O2" } else { "-g" };
    let inc_flags = env::get_include_flags();
    let mut include = include_flag(&[cache_dir, std_include_dir])?;
    include.extend(include_build_dir(build_dir)?);
    for extra in dep_include_dirs {
        include.push(format!("-I{}", extra.to_string_lossy()));
    }
    let mut args: Vec<String> = vec![opt_flag.to_string(), "-std=c++20".into(), "-c".into()];
    args.push(path_to_str(bridge_src)?.to_string());
    args.push("-o".into());
    args.push(path_to_str(bridge_obj)?.to_string());
    for flag in inc_flags.split_whitespace() {
        if !flag.is_empty() {
            args.push(flag.to_string());
        }
    }
    for flag in &include {
        args.push(flag.clone());
    }
    let mut cmd = Command::new("g++");
    cmd.args(&args);
    let bridge_output = env::run_with_timeout(&mut cmd, "g++ (bridge)", true)
        .map_err(|e| anyhow::anyhow!("Failed to compile bridge: {}", e))?;
    if !bridge_output.status.success() {
        let stderr = String::from_utf8_lossy(&bridge_output.stderr);
        eprintln!(
            "{}",
            color::error(&format!("bridge compilation failed:\n{}", stderr))
        );
        if verbose {
            eprintln!("{:?}", args);
        }
        std::process::exit(1);
    }
    Ok(())
}
