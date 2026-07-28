use super::*;

pub(crate) struct GlobalSigs {
    pub(crate) all_func_sigs: std::collections::HashMap<String, crate::codegen::FuncSig>,
    pub(crate) global_type_sigs: std::collections::HashMap<String, (Vec<String>, Vec<Param>, Option<Typ>)>,
    pub(crate) global_safety: std::collections::HashMap<String, Safety>,
    pub(crate) global_enums: std::collections::HashMap<String, Vec<crate::ast::EnumVariant>>,
}

pub(crate) fn collect_global_sigs(
    ast_cache: &mut AstCache,
    files: &[String],
    macro_table: &macro_expand::MacroTable,
    name: &str,
) -> Result<GlobalSigs> {
    // Phase 0.5: Collect function signatures from all files (cross-file type info)
    let mut all_func_sigs = std::collections::HashMap::new();
    // Qualified (module-prefixed) signatures used to resolve cross-module
    // calls during per-file type checking, e.g. `mvp_std.json.parse`.
    let mut global_type_sigs: std::collections::HashMap<
        String,
        (Vec<String>, Vec<Param>, Option<Typ>),
    > = std::collections::HashMap::new();
    // Qualified (module-prefixed) safety levels used to enforce the "cannot
    // call unsafe function from safe function" rule across module boundaries,
    // e.g. `mvp_std.json.as_string` -> unsafe. Mirrors `global_type_sigs`.
    let mut global_safety: std::collections::HashMap<String, Safety> =
        std::collections::HashMap::new();
    // Qualified (module-prefixed) enum definitions used to resolve enum
    // pattern matching across module boundaries, e.g. `Option.Some(v)`.
    let mut global_enums: std::collections::HashMap<String, Vec<crate::ast::EnumVariant>> =
        std::collections::HashMap::new();
    // Project name is used to qualify local (non-std) module calls the same
    // way the frontend does (see `util::process_call_path` / import paths).
    let pkg_name = name.to_string();
    for file in files {
        let ast = parse_cached(ast_cache, file)?;
        let defs = macro_expand::expand_macros(&ast.defs, macro_table)?;
        // Module name of this file (used to qualify its function signatures).
        let module_name = defs
            .iter()
            .find_map(|d| match d {
                crate::ast::Def::DModule { name, .. } => Some(name.clone()),
                _ => None,
            })
            .unwrap_or_default();
        for d in &defs {
            let func = match d {
                crate::ast::Def::DFunc { name, safety, .. } => Some((name, safety)),
                crate::ast::Def::DCFuncUnsafe { name, safety, .. } => Some((name, safety)),
                _ => None,
            };
            if let Some((name, safety)) = func {
                use std::collections::hash_map::Entry;
                match all_func_sigs.entry(name.clone()) {
                    Entry::Occupied(_) => {}
                    Entry::Vacant(v) => {
                        let (type_params, _params, returns, is_async, type_bounds) = match d {
                            crate::ast::Def::DFunc {
                                type_params,
                                params,
                                returns,
                                is_async,
                                type_bounds,
                                ..
                            } => (
                                type_params.clone(),
                                params.clone(),
                                returns.clone(),
                                *is_async,
                                type_bounds.clone(),
                            ),
                            crate::ast::Def::DCFuncUnsafe { returns, .. } => {
                                (Vec::new(), Vec::new(), returns.clone(), false, Vec::new())
                            }
                            _ => (Vec::new(), Vec::new(), None, false, Vec::new()),
                        };
                        v.insert(crate::codegen::FuncSig {
                            type_params,
                            returns,
                            is_async,
                            type_bounds,
                        });
                    }
                }
                let qual_prefix = qual_prefix(&module_name, file, &pkg_name);
                let qual = format!("{}.{}", qual_prefix, name);
                global_type_sigs
                    .entry(qual.clone())
                    .or_insert_with(|| {
                        let (type_params, params, returns) = match d {
                            crate::ast::Def::DFunc {
                                type_params,
                                params,
                                returns,
                                ..
                            } => {
                                // Normalize generic types so cross-module
                                // signatures use TGenericParam (not TStruct)
                                // for type parameter names — otherwise typecheck
                                // cannot resolve `T` during instantiation.
                                let norm_params =
                                    typecheck::normalize_params(params, type_params);
                                let norm_returns = returns
                                    .as_ref()
                                    .map(|r| typecheck::normalize_typ(r, type_params));
                                (type_params.clone(), norm_params, norm_returns)
                            }
                            _ => (Vec::new(), Vec::new(), None),
                        };
                        (type_params, params, returns)
                    });
                global_safety.entry(qual).or_insert_with(|| safety.clone());
            }
            // Collect enum definitions for cross-module enum pattern matching.
            if let crate::ast::Def::DEnum {
                name,
                variants,
                type_params: _,
                loc: _,
            } = d
            {
                let qual_prefix = qual_prefix(&module_name, file, &pkg_name);
                let qual = format!("{}.{}", qual_prefix, name);
                global_enums.entry(qual.clone()).or_insert_with(|| variants.clone());
                global_enums.entry(name.clone()).or_insert_with(|| variants.clone());
            }
        }
    }
    Ok(GlobalSigs {
        all_func_sigs,
        global_type_sigs,
        global_safety,
        global_enums,
    })
}

/// Qualified key mirrors the frontend's call-path rewriting
/// (`util::process_call_path`):
///   - `std.x.y`      -> `mvp_std.x.y`
///   - `main`         -> `main.y`
///   - local module   -> `<pkg>.{path under src}.y`, e.g.
///     `import "pkg/lib"` yields call `pkg.lib.y`.
fn qual_prefix(module_name: &str, file: &str, pkg_name: &str) -> String {
    if module_name == "main" {
        "main".to_string()
    } else if module_name.starts_with("std") {
        format!("mvp_{}", module_name)
    } else {
        let local = file
            .strip_prefix("src/")
            .unwrap_or(file)
            .strip_suffix(".miva")
            .unwrap_or(file)
            .replace('/', ".");
        format!("{}.{}", pkg_name, local)
    }
}
