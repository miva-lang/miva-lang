use crate::ast::*;
use crate::error::Error;
use std::collections::{HashMap, HashSet};

mod builtins;
mod generics;
mod infer;
mod lambda_capture;
mod seal;
mod shape;
#[cfg(test)]
mod tests;

pub(crate) use builtins::*;
pub(crate) use generics::*;
pub(crate) use infer::*;
pub(crate) use lambda_capture::*;
pub(crate) use seal::*;
pub(crate) use shape::*;

#[derive(Clone)]
pub(crate) struct TypeEnv {
    vars: HashMap<String, Typ>,
}

fn basename(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

fn types_equal(a: &Typ, b: &Typ) -> bool {
    // Generic-param equivalence: a bare `TStruct{name:"T"}` (no fields, no
    // type args) that names a generic type parameter is the same type as the
    // normalized `TGenericParam{name:"T"}` form. This lets the rest of type
    // checking treat the two shapes — produced by different code paths
    // (let-annotation parsing keeps TStruct; func_sigs normalize to
    // TGenericParam) — as interchangeable without chasing every producer.
    if let (Typ::TStruct { name: n1, fields: f1, type_args: ta1 },
            Typ::TGenericParam { name: n2 }) = (a, b) {
        if n1 == n2 && f1.is_empty() && ta1.is_empty() {
            return true;
        }
    }
    if let (Typ::TGenericParam { name: n1 },
            Typ::TStruct { name: n2, fields: f2, type_args: ta2 }) = (a, b) {
        if n1 == n2 && f2.is_empty() && ta2.is_empty() {
            return true;
        }
    }
    match (a, b) {
        (
            Typ::TStruct {
                name: n1,
                type_args: ta1,
                ..
            },
            Typ::TStruct {
                name: n2,
                type_args: ta2,
                ..
            },
        ) => {
            // Compare by the type's base name so that a cross-module reference
            // (e.g. `std::option::Option`) and the same type produced inside
            // its own module (e.g. `Option`) are treated as equal. Module
            // qualification is purely a namespacing prefix; the underlying
            // type identity is the final path segment.
            basename(n1) == basename(n2)
                && ta1.len() == ta2.len()
                && ta1
                    .iter()
                    .zip(ta2.iter())
                    .all(|(t1, t2)| types_equal(t1, t2))
        }
        (Typ::TArray { of: o1 }, Typ::TArray { of: o2 }) => types_equal(o1, o2),
        (Typ::TPtr { to: t1 }, Typ::TPtr { to: t2 }) => types_equal(t1, t2),
        (Typ::TBox { of: o1 }, Typ::TBox { of: o2 }) => types_equal(o1, o2),
        (Typ::TFuture { of: o1 }, Typ::TFuture { of: o2 }) => types_equal(o1, o2),
        (Typ::TGenericParam { name: n1 }, Typ::TGenericParam { name: n2 }) => n1 == n2,
        _ => a == b,
    }
}

fn is_numeric(t: &Typ) -> bool {
    matches!(t, Typ::TInt | Typ::TFloat32 | Typ::TFloat64)
}

fn build_struct_map(
    defs: &[Def],
) -> (HashMap<String, Vec<FieldDef>>, HashMap<String, Vec<String>>) {
    let mut structs = HashMap::new();
    let mut struct_type_params = HashMap::new();
    for def in defs {
        if let Def::DStruct {
            name,
            fields,
            type_params,
            ..
        } = def
        {
            let normalized = if type_params.is_empty() {
                fields.clone()
            } else {
                fields
                    .iter()
                    .map(|f| FieldDef {
                        name: f.name.clone(),
                        typ: normalize_typ(&f.typ, type_params),
                    })
                    .collect()
            };
            structs.insert(name.clone(), normalized);
            struct_type_params.insert(name.clone(), type_params.clone());
        }
    }
    (structs, struct_type_params)
}

fn build_shape_map(
    defs: &[Def],
) -> (HashMap<String, Vec<FieldDef>>, HashMap<String, Vec<String>>) {
    let mut shapes = HashMap::new();
    let mut shape_type_params = HashMap::new();
    for def in defs {
        if let Def::DShape {
            name,
            fields,
            type_params,
            ..
        } = def
        {
            let normalized = if type_params.is_empty() {
                fields.clone()
            } else {
                fields
                    .iter()
                    .map(|f| FieldDef {
                        name: f.name.clone(),
                        typ: normalize_typ(&f.typ, type_params),
                    })
                    .collect()
            };
            shapes.insert(name.clone(), normalized);
            shape_type_params.insert(name.clone(), type_params.clone());
        }
    }
    (shapes, shape_type_params)
}

fn build_enum_maps(
    defs: &[Def],
) -> (
    HashMap<String, Vec<crate::ast::EnumVariant>>,
    HashMap<String, Vec<String>>,
) {
    let mut enums = HashMap::new();
    let mut enum_type_params = HashMap::new();
    for def in defs {
        if let Def::DEnum {
            name,
            variants,
            type_params,
            ..
        } = def
        {
            enums.insert(name.clone(), variants.clone());
            enum_type_params.insert(name.clone(), type_params.clone());
        }
    }
    (enums, enum_type_params)
}

fn require_type(
    env: &mut TypeEnv,
    func_sigs: &HashMap<String, (Vec<String>, Vec<Param>, Option<Typ>)>,
    structs: &HashMap<String, Vec<FieldDef>>,
    struct_type_params: &HashMap<String, Vec<String>>,
    enums: &HashMap<String, Vec<crate::ast::EnumVariant>>,
    enum_type_params: &HashMap<String, Vec<String>>,
    func_return: &Option<Typ>,
    droppable: &HashSet<String>,
    e: &Expr,
) -> (Typ, Vec<Error>) {
    let (typ, errs) = infer_type(
        env,
        func_sigs,
        structs,
        struct_type_params,
        enums,
        enum_type_params,
        func_return, droppable,
        e,
    );
    if matches!(typ, Typ::TNull | Typ::TInvalid) {
        let mut all_errs = errs;
        all_errs.push(Error::new(
            "E0014",
            &loc_of(e),
            "expression has no value (void) but a value was expected",
        ));
        return (typ, all_errs);
    }
    (typ, errs)
}

fn loc_of(e: &Expr) -> Loc {
    match e {
        Expr::EInt { loc, .. }
        | Expr::EBool { loc, .. }
        | Expr::EFloat { loc, .. }
        | Expr::EChar { loc, .. }
        | Expr::EString { loc, .. }
        | Expr::EVar { loc, .. }
        | Expr::EMove { loc, .. }
        | Expr::EClone { loc, .. }
        | Expr::EStructLit { loc, .. }
        | Expr::EFieldAccess { loc, .. }
        | Expr::EBinOp { loc, .. }
        | Expr::EIf { loc, .. }
        | Expr::EChoose { loc, .. }
        | Expr::ECall { loc, .. }
        | Expr::EMacro { loc, .. }
        | Expr::ECast { loc, .. }
        | Expr::EBlock { loc, .. }
        | Expr::EArrayLit { loc, .. }
        | Expr::EVoid { loc, .. }
        | Expr::EAddr { loc, .. }
        | Expr::EDeref { loc, .. }
        | Expr::EWhile { loc, .. }
        | Expr::ELoop { loc, .. }
        | Expr::EFor { loc, .. }
        | Expr::EEnumPattern { loc, .. }
        | Expr::ELambda { loc, .. }
        | Expr::ETupleLit { loc, .. } => loc.clone(),
        Expr::EMacroVar { loc, .. } => loc.clone(),
        Expr::EMethodCall { loc, .. } => loc.clone(),
    }
}


// Drop functions registered via `op_drop` are sealed: only the compiler's
// desugaring pass may invoke them, so any user-written call or value use is
// rejected (E0034).
pub fn check_program(defs: &[Def]) -> Vec<Error> {
    check_program_with(
        defs,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    )
}

/// Like [`check_program`], but also consults `global` — a map of
/// fully-qualified call paths (e.g. `"mvp_std.json.parse"`) to their
/// signatures, collected from every module in the build. This lets
/// module-qualified cross-module calls (`std.json.parse`, `l2.helper.foo`,
/// ...) resolve during type checking instead of falling back to `TNull`.
pub fn check_program_with(
    defs: &[Def],
    global: &HashMap<String, (Vec<String>, Vec<Param>, Option<Typ>)>,
    global_enums: &HashMap<String, Vec<crate::ast::EnumVariant>>,
    global_shapes: &HashMap<String, Vec<FieldDef>>,
    global_shape_type_params: &HashMap<String, Vec<String>>,
) -> Vec<Error> {
    let (mut func_sigs, mut func_type_bounds) = build_func_sigs(defs);
    // Merge cross-module signatures (qualified keys). Local (per-file)
    // signatures take precedence on collisions.
    for (k, v) in global {
        if !func_sigs.contains_key(k) {
            func_sigs.insert(k.clone(), v.clone());
        }
    }
    let (structs, struct_type_params) = build_struct_map(defs);
    let (shapes, shape_type_params) = build_shape_map(defs);
    let (enums, enum_type_params) = build_enum_maps(defs);
    let droppable = crate::droppable::compute_droppable(defs);
    let has_imports = defs.iter().any(|d| {
        matches!(d, Def::SImport { .. })
            || matches!(d, Def::SImportAs { .. })
            || matches!(d, Def::SImportHere { .. })
    });
    let mut errs = vec![];

    let mut drop_registry: HashMap<String, String> = HashMap::new();
    for def in defs {
        if let Def::DImpl {
            struct_name, impls, ..
        } = def
        {
            for imp in impls {
                if !matches!(imp.op, ImplOp::ImDrop) {
                    continue;
                }
                if let Some(prev) = drop_registry.get(struct_name) {
                    errs.push(Error::new(
                        "E0032",
                        &imp.loc,
                        &format!(
                            "duplicate op_drop registration for struct '{}': already registered to '{}'",
                            struct_name, prev
                        ),
                    ));
                    continue;
                }
                match func_sigs.get(&imp.func) {
                    None => {
                        errs.push(Error::new(
                            "E0031",
                            &imp.loc,
                            &format!("op_drop function '{}' is not defined", imp.func),
                        ));
                    }
                    Some((_, params, returns)) => {
                        let param_ok = params.len() == 1
                            && matches!(
                                &params[0],
                                Param::PRef { typ: Typ::TStruct { name, .. }, .. } if name == struct_name
                            );
                        let return_ok = matches!(returns, None | Some(Typ::TNull));
                        if !param_ok || !return_ok {
                            errs.push(Error::new(
                                "E0031",
                                &imp.loc,
                                &format!(
                                    "op_drop function '{}' must have signature (ref self: {}) with no return value",
                                    imp.func, struct_name
                                ),
                            ));
                        }
                    }
                }
                drop_registry.insert(struct_name.clone(), imp.func.clone());
            }
        }
    }

    let sealed_fns: std::collections::HashSet<&str> =
        drop_registry.values().map(|s| s.as_str()).collect();
    if !sealed_fns.is_empty() {
        for def in defs {
            if let Def::DFunc { body, .. } = def {
                seal_check_expr(body, &sealed_fns, &mut errs);
            }
        }
    }

    for def in defs {
        match def {
            Def::DFunc {
                name,
                type_params,
                params,
                returns,
                body,
                is_async,
                ..
            } if *is_async => {
                let normalized_params = if !type_params.is_empty() {
                    normalize_params(params, type_params)
                } else {
                    params.clone()
                };
                // An async function must return a future[T]. The body itself is
                // checked against the inner type T.
                let expected_inner: Option<Typ> = match returns {
                    Some(Typ::TFuture { of }) => Some((**of).clone()),
                    Some(_) => {
                        errs.push(Error::new(
                            "E0020",
                            &loc_of(body),
                            "async function must return future[T]; declared return type is not a future",
                        ));
                        None
                    }
                    None => None,
                };
                let mut env = TypeEnv {
                    vars: HashMap::new(),
                };
                for p in &normalized_params {
                    match p {
                        Param::PRef { name, typ } | Param::POwn { name, typ } => {
                            env.vars.insert(name.clone(), typ.clone());
                        }
                    }
                }
                let (body_t, mut fun_errs) = infer_type(
                    &mut env,
                    &func_sigs,
                    &structs,
                    &struct_type_params,
                    &enums,
                    &enum_type_params,
                    &expected_inner,
                    &droppable,
                    body,
                );
                errs.append(&mut fun_errs);
                if let Some(ref inner) = expected_inner {
                    if !matches!(body_t, Typ::TNull) && !types_equal(inner, &body_t) {
                        errs.push(Error::new(
                            "E0017",
                            &loc_of(body),
                            &format!(
                                "async function body has type {:?} but declared future element type is {:?}",
                                body_t, inner
                            ),
                        ));
                    }
                }
                // Callers see the function as returning future[inner]: a `return`
                // statement makes the block infer to TNull, so do not derive the
                // future element from the (null) body type.
                let eff = match &expected_inner {
                    Some(inner) => Typ::TFuture {
                        of: Box::new(inner.clone()),
                    },
                    None => Typ::TFuture {
                        of: Box::new(body_t.clone()),
                    },
                };
                func_sigs.insert(
                    name.clone(),
                    (type_params.clone(), normalized_params, Some(eff)),
                );
            }
            Def::DFunc {
                name: _,
                type_params,
                params,
                returns,
                body,
                ..
            } => {
                let mut env = TypeEnv {
                    vars: HashMap::new(),
                };
                let normalized_params = if !type_params.is_empty() {
                    normalize_params(params, type_params)
                } else {
                    params.clone()
                };
                let normalized_returns = returns.as_ref().map(|r| normalize_typ(r, type_params));
                for p in &normalized_params {
                    match p {
                        Param::PRef { name, typ } | Param::POwn { name, typ } => {
                            env.vars.insert(name.clone(), typ.clone());
                        }
                    }
                }
                let (body_t, mut fun_errs) = infer_type(
                    &mut env,
                    &func_sigs,
                    &structs,
                    &struct_type_params,
                    &enums,
                    &enum_type_params,
                    &normalized_returns,
                    &droppable,
                    body,
                );
                errs.append(&mut fun_errs);
                if let Some(ref rt) = normalized_returns {
                    if !matches!(body_t, Typ::TNull) && !types_equal(rt, &body_t) {
                        errs.push(Error::new(
                            "E0017",
                            &loc_of(body),
                            &format!(
                                "function body has type {:?} but declared return type is {:?}",
                                body_t, rt
                            ),
                        ));
                    }
                }
            }
            Def::DTest { body, .. } => {
                let mut env = TypeEnv {
                    vars: HashMap::new(),
                };
                let (_, mut fun_errs) = infer_type(
                    &mut env,
                    &func_sigs,
                    &structs,
                    &struct_type_params,
                    &enums,
                    &enum_type_params,
                    &None,
                    &droppable,
                    body,
                );
                errs.append(&mut fun_errs);
            }
            _ => {}
        }
    }

    // Shape satisfaction check: verify all SLetTyped with TShape types
    for def in defs {
        match def {
            Def::DFunc { body, .. } => {
                let mut shape_errs = check_shape_satisfaction(body, &shapes, &structs);
                errs.append(&mut shape_errs);
            }
            Def::DTest { body, .. } => {
                let mut shape_errs = check_shape_satisfaction(body, &shapes, &structs);
                errs.append(&mut shape_errs);
            }
            _ => {}
        }
    }

    if has_imports {
        // Cross-module functions aren't available for type checking.
        // Filter out "void value" errors that stem from unresolved cross-module calls.
        // The C++ compiler will catch any real type mismatches.
        errs.retain(|e| e.code != "E0014");
    }

    errs
}

