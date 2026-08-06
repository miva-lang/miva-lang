use super::*;

pub(crate) fn collect_imports_with_graph(
    ast_cache: &mut AstCache,
    file: &str,
    visited: &mut HashSet<String>,
    cache_dir: &Path,
    std_path: &str,
    graph: &mut DependencyGraph,
    name: &str,
    deps: &HashMap<String, String>,
) -> Result<()> {
    let mut file_parts: Vec<_> = file.split('/').collect();
    let _file = file;
    let file: String;
    if exists(_file)? {
        file = _file.to_string();
    } else if file_parts.is_empty() {
        color::error("cannot parse import file");
        anyhow::bail!("parsing failed")
    } else {
        let first = *file_parts.get(0).unwrap_or(&"");
        file_parts.remove(0);
        match first {
            _ if first == name => {
                file = "src/".to_string() + file_parts.join("/").as_str() + ".miva";
            }
            _ if deps.contains_key(&first.to_string()) => {
                let version = &deps[&first.to_string()];
                let inc = format!("{}", env::get_std_include_dir().display());
                file = inc
                    + "/"
                    + first
                    + "-"
                    + version
                    + "/src/"
                    + file_parts.join("/").as_str()
                    + ".miva";
            }
            _ => {
                if !deps.is_empty() {
                    anyhow::bail!("import '{}' references library '{}' which is not declared in [dependencies]", _file, first);
                }
                let inc = format!("{}", env::get_std_include_dir().display());
                file = inc + "/" + first + "/src/" + file_parts.join("/").as_str() + ".miva";
            }
        }
    }

    if !visited.insert(file.clone()) {
        return Ok(());
    }

    let file = file.as_str();
    let dep_path = dep_cache_path(cache_dir, file, std_path);
    let deps_list = read_dep_cache(&dep_path, file);

    let import_paths: Vec<String> = if let Some(deps_list) = deps_list {
        deps_list
    } else {
        let ast = parse_cached(ast_cache, file)?;
        let paths: Vec<String> = ast
            .defs
            .iter()
            .filter_map(|def| match def {
                crate::ast::Def::SImport { path, .. }
                | crate::ast::Def::SImportAs { path, .. }
                | crate::ast::Def::SImportHere { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect();

        write_dep_cache(&dep_path, &paths);
        paths
    };

    for path in &import_paths {
        graph.add_dependency(file, path);
        collect_imports_with_graph(
            ast_cache, path, visited, cache_dir, std_path, graph, name, deps,
        )?;
    }

    Ok(())
}

/// Collect all macro definitions from all source files.
///
/// Parses each file via the in-process frontend and extracts `DMacro`
/// definitions into a shared `MacroTable`. This is called once before
/// per-file compilation so that every file's macro expansion has access
/// to macros defined anywhere in the project.
pub(crate) fn collect_all_macros(
    ast_cache: &mut AstCache,
    files: &[String],
) -> macro_expand::MacroTable {
    let mut table = macro_expand::MacroTable::new();
    for file in files {
        match parse_cached(ast_cache, file) {
            Ok(ast) => {
                let file_macros = macro_expand::collect_macros(&ast.defs);
                for (name, def) in file_macros {
                    table.entry(name).or_insert(def);
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: failed to parse {} for macro collection: {}",
                    file, e
                );
            }
        }
    }
    table
}
