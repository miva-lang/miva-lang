#![allow(dead_code)]
#![allow(unused_variables)]
use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::Error;
use crate::symbol_table::SymbolTable;

#[derive(Debug, Clone, PartialEq)]
enum VarState {
    Valid,
    Moved,
}

#[derive(Debug, Clone)]
struct VarInfo {
    typ: Typ,
    state: VarState,
    is_mutable: bool,
    is_ref_param: bool,
}

#[derive(Debug, Clone)]
struct Context {
    types: HashMap<String, Vec<FieldDef>>,
    vars: HashMap<String, VarInfo>,
    caller_safety: Safety,
    global_safety: HashMap<String, Safety>,
    droppable: HashSet<String>,
}

fn is_droppable_typ(droppable: &HashSet<String>, t: &Typ) -> bool {
    matches!(t, Typ::TStruct { name, .. } if droppable.contains(name))
}

fn is_droppable_var(ctx: &Context, name: &str) -> bool {
    ctx.vars
        .get(name)
        .map_or(false, |i| is_droppable_typ(&ctx.droppable, &i.typ))
}

/// Droppable values are move-only: using one by value (assignment, own-param
/// passing, return, aggregate construction) transfers ownership implicitly.
fn consume_droppable(ctx: &mut Context, expr: &Expr) {
    if let Expr::EVar { name, .. } = expr {
        if is_droppable_var(ctx, name) {
            if let Some(info) = ctx.vars.get(name) {
                if info.is_ref_param {
                    return;
                }
                ctx.vars.insert(
                    name.clone(),
                    VarInfo {
                        state: VarState::Moved,
                        ..info.clone()
                    },
                );
            }
        }
    }
}

fn is_copy_type(types: &HashMap<String, Vec<FieldDef>>, t: &Typ) -> bool {
    match t {
        Typ::TInt | Typ::TBool | Typ::TFloat32 | Typ::TFloat64 | Typ::TChar => true,
        Typ::TString
        | Typ::TArray { .. }
        | Typ::TPtr { .. }
        | Typ::TBox { .. }
        | Typ::TFuture { .. }
        | Typ::TNull
        | Typ::TInvalid
        | Typ::TPtrAny
        | Typ::TGenericParam { .. } => false,
        Typ::TStruct { name, .. } => {
            if let Some(fields) = types.get(name) {
                fields.iter().all(|f| is_copy_type(types, &f.typ))
            } else {
                false
            }
        }
        Typ::TFunc { .. } | Typ::TShape { .. } => false,
    }
}

fn check_expr(ctx: &mut Context, symbol_table: &SymbolTable, e: &Expr) -> Vec<Error> {
    let mut errs = Vec::new();

    let mut mark_moved = |ctx: &mut Context, loc: &Loc, name: &str| {
        if let Some(info) = ctx.vars.get(name) {
            if info.state == VarState::Moved {
                errs.push(Error::new(
                    "E0001",
                    loc,
                    &format!("use of moved value {}", name),
                ));
            }
            if info.is_ref_param {
                errs.push(Error::new(
                    "E0002",
                    loc,
                    &format!("cannot move ref parameter {}", name),
                ));
            }
            ctx.vars.insert(
                name.to_string(),
                VarInfo {
                    state: VarState::Moved,
                    ..info.clone()
                },
            );
        }
    };

    match e {
        Expr::EVar { loc, name } => {
            if let Some(info) = ctx.vars.get(name) {
                if info.state == VarState::Moved {
                    errs.push(Error::new(
                        "E0001",
                        loc,
                        &format!("use of moved value {}", name),
                    ));
                }
            } else if symbol_table.lookup_enum(name).is_some() {
                // Enum type name used as a constructor namespace (e.g. Shape in Shape.Circle).
            } else {
                errs.push(Error::new(
                    "E0007",
                    loc,
                    &format!("variable '{}' not found", name),
                ));
            }
        }
        Expr::EMove { loc, name } => {
            if let Some(info) = ctx.vars.get(name) {
                mark_moved(ctx, loc, name);
            } else {
                errs.push(Error::new(
                    "E0007",
                    loc,
                    &format!("variable '{}' not found", name),
                ));
            }
        }
        Expr::EClone { loc, name } => {
            if let Some(info) = ctx.vars.get(name) {
                if info.state == VarState::Moved {
                    errs.push(Error::new(
                        "E0001",
                        loc,
                        &format!("use of moved value {}", name),
                    ));
                }
            } else {
                errs.push(Error::new(
                    "E0007",
                    loc,
                    &format!("variable '{}' not found", name),
                ));
            }
        }
        Expr::ECall {
            loc,
            name,
            type_args: _,
            args,
        } => {
            // Enum variant constructor: Name.Variant(args) (dotted form) or
            // Variant(EnumName, payload...) (method-call desugared form, where
            // the first arg is `EVar(EnumName)`). The enum prefix/name is a type
            // name, not a variable/function, so skip the unknown-function check.
            let is_enum_ctor = name
                .find('.')
                .map(|dot| symbol_table.lookup_enum(&name[..dot]).is_some())
                .unwrap_or(false)
                || args.first().map(|a| match a {
                    Expr::EVar { name: n, .. } => symbol_table.lookup_enum(n.as_str()).map_or(false, |e| e.variants.iter().any(|v| &v.name == name)),
                    _ => false,
                }).unwrap_or(false);
            let safety = symbol_table
                .get_function_safety(name)
                .or_else(|| ctx.global_safety.get(name).cloned());
            match safety {
                Some(Safety::Unsafe) => {
                    if ctx.caller_safety == Safety::Safe {
                        errs.push(Error::new(
                            "E0009",
                            loc,
                            &format!(
                                "cannot call unsafe function '{}' from safe function",
                                name
                            ),
                        ));
                    }
                }
                Some(Safety::Trusted) | Some(Safety::Safe) => {}
                None => {
                    if is_enum_ctor {
                        // enum constructor — no function lookup needed
                    } else if name.starts_with("ffi.") {
                        errs.push(Error::new(
                            "E0009",
                            loc,
                            &format!(
                                "cannot call unsafe ffi function '{}' from safe function",
                                name
                            ),
                        ));
                    } else if ctx.vars.contains_key(name.as_str()) {
                        // Variable of closure type called as function — OK.
                    } else if symbol_table.imports.is_empty() {
                        // Only report unknown function if no imports are present
                        // (cross-module functions are resolved during C++ compilation)
                        errs.push(Error::new(
                            "E0009",
                            loc,
                            &format!("unknown function: {}", name),
                        ));
                    }
                }
            }
            for (i, arg) in args.iter().enumerate() {
                errs.extend(check_expr(ctx, symbol_table, arg));
                let is_ref_arg = symbol_table
                    .lookup_function(name)
                    .and_then(|f| f.params.get(i))
                    .map_or(false, |p| matches!(p, Param::PRef { .. }));
                if !is_ref_arg {
                    consume_droppable(ctx, arg);
                }
            }
            if name == "drop" {
                let valid_target = args.len() == 1
                    && match &args[0] {
                        Expr::EVar { name: v, .. } | Expr::EMove { name: v, .. } => {
                            is_droppable_var(ctx, v)
                                || ctx.vars.get(v.as_str()).map_or(true, |info| {
                                    // Moved-state vars already erred above (E0001);
                                    // don't stack E0035 on top.
                                    info.state == VarState::Moved
                                        && is_droppable_typ(&ctx.droppable, &info.typ)
                                })
                        }
                        _ => false,
                    };
                if !valid_target {
                    errs.push(Error::new(
                        "E0035",
                        loc,
                        "drop() takes exactly one droppable variable",
                    ));
                }
            }
        }
        Expr::EStructLit { loc, fields, .. } => {
            for vf in fields {
                errs.extend(check_expr(ctx, symbol_table, &vf.value));
                consume_droppable(ctx, &vf.value);
            }
        }
        Expr::EArrayLit { loc, values } => {
            for elem in values {
                errs.extend(check_expr(ctx, symbol_table, elem));
                consume_droppable(ctx, elem);
            }
        }
        Expr::EFieldAccess { loc, expr, field } => {
            // Enum variant discriminant: Name.Variant (used in `when (Name.Variant)`).
            // `Name` is an enum type name, not a variable, so don't recurse into it.
            let is_enum_discriminant = matches!(
                expr.as_ref(),
                Expr::EVar { name, .. } if symbol_table.lookup_enum(name).map_or(false, |e| e.variants.iter().any(|v| &v.name == field))
            );
            if !is_enum_discriminant {
                errs.extend(check_expr(ctx, symbol_table, expr));
            }
        }
        Expr::EBinOp {
            loc, left, right, ..
        } => {
            errs.extend(check_expr(ctx, symbol_table, left));
            errs.extend(check_expr(ctx, symbol_table, right));
        }
        Expr::EIf {
            loc,
            cond,
            then,
            else_,
            ..
        } => {
            errs.extend(check_expr(ctx, symbol_table, cond));

            let vars_before = ctx.vars.clone();
            let vars_before_snapshot = vars_before.clone();

            errs.extend(check_expr(ctx, symbol_table, then));
            let vars_after_then = ctx.vars.clone();

            ctx.vars = vars_before.clone();
            let vars_after_else = match else_ {
                Some(e) => {
                    errs.extend(check_expr(ctx, symbol_table, e));
                    ctx.vars.clone()
                }
                None => vars_before,
            };

            let merged_vars = merge_var_maps(&vars_after_then, &vars_after_else);
            ctx.vars = merged_vars;

            for (name, info) in vars_before_snapshot.iter() {
                if info.state == VarState::Valid
                    && is_droppable_typ(&ctx.droppable, &info.typ)
                    && !info.is_ref_param
                {
                    let then_state = vars_after_then.get(name).map(|i| i.state.clone());
                    let else_state = vars_after_else.get(name).map(|i| i.state.clone());
                    if then_state != else_state {
                        errs.push(Error::new(
                            "E0033",
                            loc,
                            &format!(
                                "droppable value '{}' is moved in only one branch; move it in both branches or neither",
                                name
                            ),
                        ));
                    }
                }
            }
        }
        Expr::EChoose {
            loc,
            var,
            cases,
            otherwise,
            ..
        } => {
            if otherwise.is_none() {
                errs.push(Error::new(
                    "E0011",
                    loc,
                    "choose expression must have an otherwise branch",
                ));
            }

            errs.extend(check_expr(ctx, symbol_table, var));

            let vars_before = ctx.vars.clone();

            let mut branch_vars = Vec::new();
            for case in cases {
                ctx.vars = vars_before.clone();
                errs.extend(check_expr(ctx, symbol_table, &case.when));
                if let Expr::EEnumPattern {
                    enum_name,
                    variant,
                    bindings,
                    loc,
                } = case.when.as_ref()
                {
                    match symbol_table.lookup_enum(enum_name) {
                        Some(e) => match e.variants.iter().find(|v| &v.name == variant) {
                            Some(v) => {
                                if v.payload.len() != bindings.len() {
                                    errs.push(Error::new(
                                        "E0016",
                                        loc,
                                        &format!(
                                            "enum variant '{}' expects {} binding(s), got {}",
                                            variant,
                                            v.payload.len(),
                                            bindings.len()
                                        ),
                                    ));
                                }
                                for b in bindings {
                                    ctx.vars.insert(
                                        b.clone(),
                                        VarInfo {
                                            typ: Typ::TInt,
                                            state: VarState::Valid,
                                            is_mutable: false,
                                            is_ref_param: false,
                                        },
                                    );
                                }
                            }
                            None => errs.push(Error::new(
                                "E0019",
                                loc,
                                &format!("unknown variant '{}' in enum '{}'", variant, enum_name),
                            )),
                        },
                        None => errs.push(Error::new(
                            "E0018",
                            loc,
                            &format!("unknown enum '{}'", enum_name),
                        )),
                    }
                }
                errs.extend(check_expr(ctx, symbol_table, &case.then));
                if let Some(g) = &case.guard {
                    errs.extend(check_expr(ctx, symbol_table, g));
                }
                branch_vars.push(ctx.vars.clone());
            }

            ctx.vars = vars_before.clone();
            let otherwise_vars = match otherwise {
                Some(body) => {
                    errs.extend(check_expr(ctx, symbol_table, body));
                    ctx.vars.clone()
                }
                None => vars_before.clone(),
            };

            let all_branch_vars: Vec<_> = branch_vars
                .into_iter()
                .chain(Some(otherwise_vars))
                .collect();

            for (name, info) in vars_before.iter() {
                if info.state == VarState::Valid
                    && is_droppable_typ(&ctx.droppable, &info.typ)
                    && !info.is_ref_param
                {
                    let states: Vec<_> = all_branch_vars
                        .iter()
                        .map(|m| m.get(name).map(|i| i.state.clone()))
                        .collect();
                    if states.windows(2).any(|w| w[0] != w[1]) {
                        errs.push(Error::new(
                            "E0033",
                            loc,
                            &format!(
                                "droppable value '{}' is moved in only some branches; move it in all branches or none",
                                name
                            ),
                        ));
                    }
                }
            }

            let merged_vars = all_branch_vars
                .into_iter()
                .fold(vars_before.clone(), |acc, branch_vars| {
                    merge_var_maps(&acc, &branch_vars)
                });
            ctx.vars = merged_vars;
        }
        Expr::EBlock {
            loc: _,
            stmts,
            result,
            ..
        } => {
            let saved_vars = ctx.vars.clone();

            for stmt in stmts {
                match stmt {
                    Stmt::SLet {
                        loc,
                        mutable,
                        name,
                        expr,
                    } => {
                        errs.extend(check_expr(ctx, symbol_table, expr));
                        consume_droppable(ctx, expr);
                        let inferred = match expr.as_ref() {
                            Expr::EStructLit { name: sn, .. } => Typ::TStruct {
                                name: sn.clone(),
                                fields: vec![],
                                type_args: vec![],
                            },
                            Expr::EVar { name: v, .. }
                            | Expr::EMove { name: v, .. }
                            | Expr::EClone { name: v, .. } => ctx
                                .vars
                                .get(v)
                                .map(|i| i.typ.clone())
                                .unwrap_or(Typ::TInt),
                            Expr::ECall { name: f, .. } => symbol_table
                                .lookup_function(f)
                                .and_then(|e| e.return_typ.clone())
                                .unwrap_or(Typ::TInt),
                            _ => Typ::TInt,
                        };
                        ctx.vars.insert(
                            name.clone(),
                            VarInfo {
                                typ: inferred,
                                state: VarState::Valid,
                                is_mutable: *mutable,
                                is_ref_param: false,
                            },
                        );
                    }
                    Stmt::SLetTyped {
                        loc,
                        name,
                        typ,
                        expr,
                    } => {
                        errs.extend(check_expr(ctx, symbol_table, expr));
                        consume_droppable(ctx, expr);
                        ctx.vars.insert(
                            name.clone(),
                            VarInfo {
                                typ: typ.clone(),
                                state: VarState::Valid,
                                is_mutable: false,
                                is_ref_param: false,
                            },
                        );
                    }
                    Stmt::SAssign { loc, name, expr } => {
                        if let Some(var_info) = ctx.vars.get(name.as_str()) {
                            let is_mutable = var_info.is_mutable;
                            let is_moved = var_info.state == VarState::Moved;
                            let var_info_clone = var_info.clone();
                            if !is_mutable {
                                errs.push(Error::new(
                                    "E0002",
                                    loc,
                                    &format!("cannot assign to immutable variable: {}", name),
                                ));
                            }
                            if is_moved {
                                errs.push(Error::new(
                                    "E0001",
                                    loc,
                                    &format!("use of moved value {}", name),
                                ));
                            }
                            errs.extend(check_expr(ctx, symbol_table, expr));
                            consume_droppable(ctx, expr);
                            ctx.vars.insert(
                                name.clone(),
                                VarInfo {
                                    state: VarState::Valid,
                                    ..var_info_clone
                                },
                            );
                        }
                    }
                    Stmt::SReturn { loc, expr } => {
                        errs.extend(check_expr(ctx, symbol_table, expr));
                        consume_droppable(ctx, expr);
                    }
                    Stmt::SFieldAssign { loc, target, expr, .. } => {
                        errs.extend(check_expr(ctx, symbol_table, target));
                        errs.extend(check_expr(ctx, symbol_table, expr));
                    }
                    Stmt::SExpr { loc, expr } => {
                        errs.extend(check_expr(ctx, symbol_table, expr));
                    }
                    Stmt::SCIntro { .. } | Stmt::SEmpty { .. } => {}
                }
            }

            if let Some(e) = result {
                errs.extend(check_expr(ctx, symbol_table, e));
                consume_droppable(ctx, e);
            }

            let propagated_vars = merge_saved_with_current(&saved_vars, &ctx.vars);
            ctx.vars = propagated_vars;
        }
        Expr::ECast { loc, expr, .. } => {
            errs.extend(check_expr(ctx, symbol_table, expr));
        }
        Expr::EDeref { loc, .. } => {
            // Pointer dereference is allowed inside `unsafe` (and `trusted`)
            // functions; only flag it when the enclosing function is safe.
            if ctx.caller_safety == Safety::Safe {
                errs.push(Error::new(
                    "E0010",
                    loc,
                    "cannot dereference a ptr in a safe function.",
                ));
            }
        }
        Expr::EWhile { loc, cond, body } => {
            errs.extend(check_expr(ctx, symbol_table, cond));
            errs.extend(check_expr(ctx, symbol_table, body));
        }
        Expr::ELoop { loc, body } => {
            errs.extend(check_expr(ctx, symbol_table, body));
        }
        Expr::EFor {
            loc,
            var,
            range,
            body,
        } => {
            errs.extend(check_expr(ctx, symbol_table, range));
            ctx.vars.insert(
                var.clone(),
                VarInfo {
                    typ: Typ::TInt,
                    state: VarState::Valid,
                    is_mutable: false,
                    is_ref_param: false,
                },
            );
            errs.extend(check_expr(ctx, symbol_table, body));
        }
        Expr::EInt { .. }
        | Expr::EBool { .. }
        | Expr::EFloat { .. }
        | Expr::EChar { .. }
        | Expr::EString { .. }
        | Expr::EVoid { .. }
        | Expr::EMacro { .. }
        | Expr::EMacroVar { .. }
        | Expr::EAddr { .. }
        | Expr::EEnumPattern { .. }
        | Expr::ELambda { .. } => {}
        Expr::EMethodCall { .. } => unreachable!(),
    }

    errs
}

fn merge_var_maps(
    a: &HashMap<String, VarInfo>,
    b: &HashMap<String, VarInfo>,
) -> HashMap<String, VarInfo> {
    let mut result = HashMap::new();
    let all_keys: std::collections::HashSet<&String> = a.keys().chain(b.keys()).collect();
    for key in all_keys {
        match (a.get(key), b.get(key)) {
            (Some(info1), Some(info2)) => {
                if info1.state == VarState::Moved || info2.state == VarState::Moved {
                    result.insert(
                        key.clone(),
                        VarInfo {
                            state: VarState::Moved,
                            ..info1.clone()
                        },
                    );
                } else {
                    result.insert(key.clone(), info1.clone());
                }
            }
            (Some(info), None) | (None, Some(info)) => {
                result.insert(key.clone(), info.clone());
            }
            (None, None) => {}
        }
    }
    result
}

fn merge_saved_with_current(
    saved: &HashMap<String, VarInfo>,
    current: &HashMap<String, VarInfo>,
) -> HashMap<String, VarInfo> {
    let mut result = HashMap::new();
    let all_keys: std::collections::HashSet<&String> = saved.keys().chain(current.keys()).collect();
    for key in all_keys {
        match (saved.get(key), current.get(key)) {
            (Some(saved_info), Some(current_info)) => {
                if current_info.state == VarState::Moved {
                    result.insert(
                        key.clone(),
                        VarInfo {
                            state: VarState::Moved,
                            ..saved_info.clone()
                        },
                    );
                } else {
                    result.insert(key.clone(), saved_info.clone());
                }
            }
            (Some(saved_info), None) => {
                result.insert(key.clone(), saved_info.clone());
            }
            (None, Some(_)) => {}
            (None, None) => {}
        }
    }
    result
}

pub fn check_program_with(
    defs: &[Def],
    global_safety: &HashMap<String, Safety>,
    global_enums: &HashMap<String, Vec<crate::ast::EnumVariant>>,
) -> Vec<Error> {
    let (mut symbol_table, mut errs) = SymbolTable::build_with_errors(defs);

    for (name, variants) in global_enums {
        symbol_table.register_global_enum(name, &[], variants);
    }

    let types: HashMap<String, Vec<FieldDef>> = defs
        .iter()
        .filter_map(|d| match d {
            Def::DStruct { name, fields, .. } => Some((name.clone(), fields.clone())),
            _ => None,
        })
        .collect();

    let droppable: HashSet<String> = defs
        .iter()
        .filter_map(|d| match d {
            Def::DImpl {
                struct_name, impls, ..
            } if impls.iter().any(|i| matches!(i.op, ImplOp::ImDrop)) => {
                Some(struct_name.clone())
            }
            _ => None,
        })
        .collect();

    for def in defs {
        match def {
            Def::DModule { loc, name } => {
                let module_decls: Vec<_> = defs
                    .iter()
                    .filter_map(|d| match d {
                        Def::DModule { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                    .collect();
                let has_non_module_before = defs
                    .iter()
                    .position(|d| matches!(d, Def::DModule { .. }))
                    .map_or(false, |pos| pos > 0);
                if has_non_module_before {
                    errs.push(Error::new(
                        "E0005",
                        loc,
                        &format!("module declaration must be at the top of the file"),
                    ));
                }
                if module_decls.iter().filter(|n| *n == name).count() > 1 {
                    errs.push(Error::new("E0005", loc, "duplicate module declaration"));
                }
                if module_decls.len() > 1 {
                    errs.push(Error::new(
                        "E0005",
                        loc,
                        "program must have only one module declaration",
                    ));
                }
            }
            Def::DFunc {
                loc: _,
                name: _,
                params,
                body,
                safety,
                ..
            } => {
                let vars: HashMap<String, VarInfo> = params
                    .iter()
                    .map(|p| match p {
                        Param::PRef { name, typ } => (
                            name.clone(),
                            VarInfo {
                                typ: typ.clone(),
                                state: VarState::Valid,
                                is_mutable: false,
                                is_ref_param: true,
                            },
                        ),
                        Param::POwn { name, typ } => (
                            name.clone(),
                            VarInfo {
                                typ: typ.clone(),
                                state: VarState::Valid,
                                is_mutable: false,
                                is_ref_param: false,
                            },
                        ),
                    })
                    .collect();
                let mut ctx = Context {
                    types: types.clone(),
                    vars,
                    caller_safety: safety.clone(),
                    global_safety: global_safety.clone(),
                    droppable: droppable.clone(),
                };
                errs.extend(check_expr(&mut ctx, &symbol_table, body));
            }
            Def::DCFuncUnsafe { .. } => {}
            Def::DStruct { .. } => {}
            Def::DTest {
                loc: _,
                name: _,
                body,
            } => {
                let mut ctx = Context {
                    types: types.clone(),
                    vars: HashMap::new(),
                    caller_safety: Safety::Safe,
                    global_safety: global_safety.clone(),
                    droppable: droppable.clone(),
                };
                errs.extend(check_expr(&mut ctx, &symbol_table, body));
            }
            Def::DCMagical { loc, content } => {
                let s = content.trim();
                let sd: Vec<&str> = s.split(' ').collect();
                if sd.len() < 2 {
                    errs.push(Error::new("E0013", loc, "magical comments isn't valid"));
                } else {
                    match sd[0] {
                        "warning_off" | "warning_err" | "release" | "mangle" => {}
                        _ => {
                            errs.push(Error::new(
                                "E0013",
                                loc,
                                &format!("invalid magical comment {}", sd[0]),
                            ));
                        }
                    }
                }
            }
            Def::DMacro { .. }
            | Def::SExport { .. }
            | Def::SImport { .. }
            | Def::SImportAs { .. }
            | Def::SImportHere { .. }
            | Def::DCIntro { .. }
            | Def::DImpl { .. }
            | Def::DEnum { .. }
            | Def::DShape { .. } => {}
        }
    }

    let has_module_decl = defs.iter().any(|d| matches!(d, Def::DModule { .. }));
    if !has_module_decl {
        errs.push(Error::new(
            "E0005",
            &Loc { line: 0, col: 0 },
            "program must have one module declaration",
        ));
    }

    errs
}

pub fn check_program(defs: &[Def]) -> Vec<Error> {
    check_program_with(defs, &HashMap::new(), &HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc() -> Loc {
        Loc { line: 1, col: 1 }
    }

    fn make_module(name: &str) -> Def {
        Def::DModule {
            loc: loc(),
            name: name.to_string(),
        }
    }

    fn make_func(name: &str, params: Vec<Param>, body: Expr, safety: Safety) -> Def {
        Def::DFunc {
            loc: loc(),
            name: name.to_string(),
            type_params: vec![],
            params,
            returns: None,
            body: Box::new(body),
            safety,
            is_async: false,
            type_bounds: vec![],
        }
    }

    fn make_test_def(name: &str, body: Expr) -> Def {
        Def::DTest {
            loc: loc(),
            name: name.to_string(),
            body: Box::new(body),
        }
    }

    fn make_struct(name: &str, fields: Vec<FieldDef>) -> Def {
        Def::DStruct {
            loc: loc(),
            name: name.to_string(),
            type_params: vec![],
            fields,
        }
    }

    #[test]
    fn test_empty_program_has_module_error() {
        let errs = check_program(&[]);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0005"));
    }

    #[test]
    fn test_valid_program_no_errors() {
        let defs = vec![
            make_module("test"),
            make_func("main", vec![], Expr::EVoid { loc: loc() }, Safety::Safe),
        ];
        let errs = check_program(&defs);
        assert!(errs.is_empty(), "Expected no errors, got: {:?}", errs);
    }

    #[test]
    fn test_use_of_undefined_variable() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EVar {
                    loc: loc(),
                    name: "undefined_var".to_string(),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0007"));
    }

    #[test]
    fn test_move_then_use() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![Param::POwn {
                    name: "x".to_string(),
                    typ: Typ::TInt,
                }],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EMove {
                                loc: loc(),
                                name: "x".to_string(),
                            }),
                        },
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EVar {
                                loc: loc(),
                                name: "x".to_string(),
                            }),
                        },
                    ],
                    result: Some(Box::new(Expr::EVoid { loc: loc() })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0001"));
    }

    #[test]
    fn test_unknown_function_call() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::ECall {
                    loc: loc(),
                    name: "nonexistent_func".to_string(),
                    type_args: vec![],
                    args: vec![],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0009"));
    }

    #[test]
    fn test_unsafe_function_call_from_safe() {
        let defs = vec![
            make_module("test"),
            make_func(
                "dangerous",
                vec![],
                Expr::EVoid { loc: loc() },
                Safety::Unsafe,
            ),
            make_func(
                "main",
                vec![],
                Expr::ECall {
                    loc: loc(),
                    name: "dangerous".to_string(),
                    type_args: vec![],
                    args: vec![],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0009"));
        assert!(errs.iter().any(|e| e.message.contains("unsafe function")));
    }

    #[test]
    fn test_deref_in_safe_function() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EDeref {
                    loc: loc(),
                    expr: Box::new(Expr::EVoid { loc: loc() }),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0010"));
    }

    #[test]
    fn test_choose_without_otherwise() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EChoose {
                    loc: loc(),
                    var: Box::new(Expr::EInt {
                        loc: loc(),
                        value: 1,
                    }),
                    cases: vec![WhenCase {
                        when: Box::new(Expr::EInt {
                            loc: loc(),
                            value: 1,
                        }),
                        guard: None,
                        then: Box::new(Expr::EVoid { loc: loc() }),
                    }],
                    otherwise: None,
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0011"));
    }

    #[test]
    fn test_assign_to_immutable() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SLet {
                            loc: loc(),
                            mutable: false,
                            name: "x".to_string(),
                            expr: Box::new(Expr::EInt {
                                loc: loc(),
                                value: 1,
                            }),
                        },
                        Stmt::SAssign {
                            loc: loc(),
                            name: "x".to_string(),
                            expr: Box::new(Expr::EInt {
                                loc: loc(),
                                value: 2,
                            }),
                        },
                    ],
                    result: Some(Box::new(Expr::EVoid { loc: loc() })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0002"));
    }

    #[test]
    fn test_is_copy_type_primitives() {
        let types = HashMap::new();
        assert!(is_copy_type(&types, &Typ::TInt));
        assert!(is_copy_type(&types, &Typ::TBool));
        assert!(is_copy_type(&types, &Typ::TFloat32));
        assert!(is_copy_type(&types, &Typ::TFloat64));
        assert!(is_copy_type(&types, &Typ::TChar));
    }

    #[test]
    fn test_is_copy_type_non_copy() {
        let types = HashMap::new();
        assert!(!is_copy_type(&types, &Typ::TString));
        assert!(!is_copy_type(&types, &Typ::TNull));
        assert!(!is_copy_type(&types, &Typ::TPtrAny));
    }

    #[test]
    fn test_is_copy_type_struct_all_copy() {
        let mut types = HashMap::new();
        types.insert(
            "Point".to_string(),
            vec![
                FieldDef {
                    name: "x".to_string(),
                    typ: Typ::TInt,
                },
                FieldDef {
                    name: "y".to_string(),
                    typ: Typ::TInt,
                },
            ],
        );
        assert!(is_copy_type(
            &types,
            &Typ::TStruct {
                name: "Point".to_string(),
                type_args: vec![],
                fields: vec![],
            }
        ));
    }

    #[test]
    fn test_is_copy_type_struct_non_copy_field() {
        let mut types = HashMap::new();
        types.insert(
            "Bad".to_string(),
            vec![FieldDef {
                name: "s".to_string(),
                typ: Typ::TString,
            }],
        );
        assert!(!is_copy_type(
            &types,
            &Typ::TStruct {
                name: "Bad".to_string(),
                type_args: vec![],
                fields: vec![],
            }
        ));
    }

    #[test]
    fn test_if_branch_move_merge() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![Param::POwn {
                    name: "x".to_string(),
                    typ: Typ::TInt,
                }],
                Expr::EIf {
                    loc: loc(),
                    cond: Box::new(Expr::EBool {
                        loc: loc(),
                        value: true,
                    }),
                    then: Box::new(Expr::EMove {
                        loc: loc(),
                        name: "x".to_string(),
                    }),
                    else_: Some(Box::new(Expr::EMove {
                        loc: loc(),
                        name: "x".to_string(),
                    })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(
            errs.is_empty(),
            "Both branches move x, so x should be considered moved after if"
        );
    }

    #[test]
    fn test_trusted_function_call_allowed() {
        let defs = vec![
            make_module("test"),
            make_func(
                "trusted_func",
                vec![],
                Expr::EVoid { loc: loc() },
                Safety::Trusted,
            ),
            make_func(
                "main",
                vec![],
                Expr::ECall {
                    loc: loc(),
                    name: "trusted_func".to_string(),
                    type_args: vec![],
                    args: vec![],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(
            errs.is_empty(),
            "Trusted function calls should be allowed from safe functions"
        );
    }

    #[test]
    fn test_safe_function_call_allowed() {
        let defs = vec![
            make_module("test"),
            make_func(
                "safe_func",
                vec![],
                Expr::EVoid { loc: loc() },
                Safety::Safe,
            ),
            make_func(
                "main",
                vec![],
                Expr::ECall {
                    loc: loc(),
                    name: "safe_func".to_string(),
                    type_args: vec![],
                    args: vec![],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(
            errs.is_empty(),
            "Safe function calls should be allowed from safe functions"
        );
    }

    #[test]
    fn test_clone_after_move_error() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![Param::POwn {
                    name: "x".to_string(),
                    typ: Typ::TInt,
                }],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EMove {
                                loc: loc(),
                                name: "x".to_string(),
                            }),
                        },
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EClone {
                                loc: loc(),
                                name: "x".to_string(),
                            }),
                        },
                    ],
                    result: Some(Box::new(Expr::EVoid { loc: loc() })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0001"));
    }

    #[test]
    fn test_ref_param_move_error() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![Param::PRef {
                    name: "x".to_string(),
                    typ: Typ::TInt,
                }],
                Expr::EMove {
                    loc: loc(),
                    name: "x".to_string(),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0002"));
    }

    #[test]
    fn test_assign_after_move_error() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![Param::POwn {
                    name: "x".to_string(),
                    typ: Typ::TInt,
                }],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EMove {
                                loc: loc(),
                                name: "x".to_string(),
                            }),
                        },
                        Stmt::SAssign {
                            loc: loc(),
                            name: "x".to_string(),
                            expr: Box::new(Expr::EInt {
                                loc: loc(),
                                value: 5,
                            }),
                        },
                    ],
                    result: Some(Box::new(Expr::EVoid { loc: loc() })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0001"));
    }

    #[test]
    fn test_mutable_assign_resets_state() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SLet {
                            loc: loc(),
                            mutable: true,
                            name: "x".to_string(),
                            expr: Box::new(Expr::EInt {
                                loc: loc(),
                                value: 1,
                            }),
                        },
                        Stmt::SAssign {
                            loc: loc(),
                            name: "x".to_string(),
                            expr: Box::new(Expr::EInt {
                                loc: loc(),
                                value: 2,
                            }),
                        },
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EVar {
                                loc: loc(),
                                name: "x".to_string(),
                            }),
                        },
                    ],
                    result: Some(Box::new(Expr::EVoid { loc: loc() })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(
            errs.is_empty(),
            "Mutable assignment should reset state to Valid"
        );
    }

    #[test]
    fn test_nested_block_scopes() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SLet {
                            loc: loc(),
                            mutable: false,
                            name: "outer".to_string(),
                            expr: Box::new(Expr::EInt {
                                loc: loc(),
                                value: 1,
                            }),
                        },
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EBlock {
                                loc: loc(),
                                stmts: vec![Stmt::SLet {
                                    loc: loc(),
                                    mutable: false,
                                    name: "inner".to_string(),
                                    expr: Box::new(Expr::EInt {
                                        loc: loc(),
                                        value: 2,
                                    }),
                                }],
                                result: Some(Box::new(Expr::EVoid { loc: loc() })),
                            }),
                        },
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EVar {
                                loc: loc(),
                                name: "outer".to_string(),
                            }),
                        },
                    ],
                    result: Some(Box::new(Expr::EVoid { loc: loc() })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(
            errs.is_empty(),
            "Outer variable should still be accessible after nested block"
        );
    }

    #[test]
    fn test_struct_lit_field_checking() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EStructLit {
                    loc: loc(),
                    name: "Point".to_string(),
                    type_args: vec![],
                    fields: vec![ValueField {
                        name: "x".to_string(),
                        value: Expr::EVar {
                            loc: loc(),
                            name: "undefined".to_string(),
                        },
                    }],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0007"));
    }

    #[test]
    fn test_array_lit_element_checking() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EArrayLit {
                    loc: loc(),
                    values: vec![
                        Expr::EInt {
                            loc: loc(),
                            value: 1,
                        },
                        Expr::EVar {
                            loc: loc(),
                            name: "undefined".to_string(),
                        },
                    ],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0007"));
    }

    #[test]
    fn test_binop_both_sides_checked() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EBinOp {
                    loc: loc(),
                    op: BinOp::Add,
                    left: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "left_undefined".to_string(),
                    }),
                    right: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "right_undefined".to_string(),
                    }),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.iter().filter(|e| e.code == "E0007").count() >= 2);
    }

    #[test]
    fn test_cast_inner_checked() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::ECast {
                    loc: loc(),
                    expr: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "undefined".to_string(),
                    }),
                    to: Typ::TInt,
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0007"));
    }

    #[test]
    fn test_field_access_inner_checked() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EFieldAccess {
                    loc: loc(),
                    expr: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "undefined".to_string(),
                    }),
                    field: "x".to_string(),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0007"));
    }

    #[test]
    fn test_while_loop_checking() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EWhile {
                    loc: loc(),
                    cond: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "undefined_cond".to_string(),
                    }),
                    body: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "undefined_body".to_string(),
                    }),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.iter().filter(|e| e.code == "E0007").count() >= 2);
    }

    #[test]
    fn test_loop_body_checked() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::ELoop {
                    loc: loc(),
                    body: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "undefined".to_string(),
                    }),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0007"));
    }

    #[test]
    fn test_for_loop_checking() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EFor {
                    loc: loc(),
                    var: "i".to_string(),
                    range: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "undefined_range".to_string(),
                    }),
                    body: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "undefined_body".to_string(),
                    }),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.iter().filter(|e| e.code == "E0007").count() >= 2);
    }

    #[test]
    fn test_for_loop_var_accessible_in_body() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EFor {
                    loc: loc(),
                    var: "i".to_string(),
                    range: Box::new(Expr::EArrayLit {
                        loc: loc(),
                        values: vec![
                            Expr::EInt {
                                loc: loc(),
                                value: 1,
                            },
                            Expr::EInt {
                                loc: loc(),
                                value: 2,
                            },
                        ],
                    }),
                    body: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "i".to_string(),
                    }),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        let var_errs: Vec<_> = errs.iter().filter(|e| e.code == "E0007").collect();
        assert!(
            var_errs.is_empty(),
            "loop variable 'i' should be accessible in body, got: {:?}",
            errs
        );
    }

    #[test]
    fn test_multiple_module_declarations_error() {
        let defs = vec![make_module("first"), make_module("second")];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0005"));
    }

    #[test]
    fn test_duplicate_module_declaration_error() {
        let defs = vec![
            make_module("test"),
            make_func("f", vec![], Expr::EVoid { loc: loc() }, Safety::Safe),
            make_module("test"),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0005"));
    }

    #[test]
    fn test_invalid_magical_comment() {
        let defs = vec![
            make_module("test"),
            Def::DCMagical {
                loc: loc(),
                content: "invalid_directive".to_string(),
            },
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0013"));
    }

    #[test]
    fn test_valid_magical_comments_no_error() {
        for directive in &[
            "warning_off foo",
            "warning_err bar",
            "release true",
            "mangle name",
        ] {
            let defs = vec![
                make_module("test"),
                Def::DCMagical {
                    loc: loc(),
                    content: directive.to_string(),
                },
            ];
            let errs = check_program(&defs);
            assert!(
                !errs.iter().any(|e| e.code == "E0013"),
                "Directive '{}' should be valid",
                directive
            );
        }
    }

    #[test]
    fn test_magical_comment_too_short() {
        let defs = vec![
            make_module("test"),
            Def::DCMagical {
                loc: loc(),
                content: "warning_off".to_string(),
            },
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0013"));
    }

    #[test]
    fn test_test_def_checks_body() {
        let defs = vec![
            make_module("test"),
            make_test_def(
                "test_foo",
                Expr::EVar {
                    loc: loc(),
                    name: "undefined_in_test".to_string(),
                },
            ),
        ];
        let errs = check_program(&defs);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "E0007"));
    }

    #[test]
    fn test_call_with_args_checks_each_arg() {
        let defs = vec![
            make_module("test"),
            make_func("helper", vec![], Expr::EVoid { loc: loc() }, Safety::Safe),
            make_func(
                "main",
                vec![],
                Expr::ECall {
                    loc: loc(),
                    name: "helper".to_string(),
                    type_args: vec![],
                    args: vec![
                        Expr::EVar {
                            loc: loc(),
                            name: "undefined1".to_string(),
                        },
                        Expr::EVar {
                            loc: loc(),
                            name: "undefined2".to_string(),
                        },
                    ],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.iter().filter(|e| e.code == "E0007").count() >= 2);
    }

    #[test]
    fn test_builtin_function_call_known() {
        for builtin in &[
            "print",
            "println",
            "string_concat",
            "box_new",
            "range",
            "exit",
            "abort",
            "panic",
        ] {
            let defs = vec![
                make_module("test"),
                make_func(
                    "main",
                    vec![],
                    Expr::ECall {
                        loc: loc(),
                        name: builtin.to_string(),
                        type_args: vec![],
                        args: vec![],
                    },
                    Safety::Safe,
                ),
            ];
            let errs = check_program(&defs);
            let unknown_errs: Vec<_> = errs
                .iter()
                .filter(|e| e.code == "E0009" && e.message.contains("unknown function"))
                .collect();
            assert!(
                unknown_errs.is_empty(),
                "builtin '{}' should not be unknown, errors: {:?}",
                builtin,
                errs
            );
        }
    }

    #[test]
    fn test_builtin_ptr_set_unsafe() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::ECall {
                    loc: loc(),
                    name: "ptr_set".to_string(),
                    type_args: vec![],
                    args: vec![],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs
            .iter()
            .any(|e| e.code == "E0009" && e.message.contains("unsafe function")));
    }

    #[test]
    fn test_user_function_overrides_builtin() {
        let defs = vec![
            make_module("test"),
            make_func("print", vec![], Expr::EVoid { loc: loc() }, Safety::Safe),
            make_func(
                "main",
                vec![],
                Expr::ECall {
                    loc: loc(),
                    name: "print".to_string(),
                    type_args: vec![],
                    args: vec![],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        let unknown_errs: Vec<_> = errs
            .iter()
            .filter(|e| e.code == "E0009" && e.message.contains("unknown function"))
            .collect();
        assert!(
             unknown_errs.is_empty(),
             "user-defined 'print' should override builtin, errors: {:?}",
             errs
         );
    }

    #[test]
    fn test_clone_prevents_move_error() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![Param::POwn {
                    name: "x".to_string(),
                    typ: Typ::TInt,
                }],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SLet {
                            loc: loc(),
                            mutable: true,
                            name: "y".to_string(),
                            expr: Box::new(Expr::EClone {
                                loc: loc(),
                                name: "x".to_string(),
                            }),
                        },
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EMove {
                                loc: loc(),
                                name: "x".to_string(),
                            }),
                        },
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EVar {
                                loc: loc(),
                                name: "y".to_string(),
                            }),
                        },
                    ],
                    result: Some(Box::new(Expr::EVoid { loc: loc() })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.is_empty(), "clone should prevent move error, got: {:?}", errs);
    }

    #[test]
    fn test_multiple_unsafe_calls_from_safe() {
        let defs = vec![
            make_module("test"),
            make_func("unsafe_fn1", vec![], Expr::EVoid { loc: loc() }, Safety::Unsafe),
            make_func("unsafe_fn2", vec![], Expr::EVoid { loc: loc() }, Safety::Unsafe),
            make_func(
                "main",
                vec![],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::ECall {
                                loc: loc(),
                                name: "unsafe_fn1".to_string(),
                                type_args: vec![],
                                args: vec![],
                            }),
                        },
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::ECall {
                                loc: loc(),
                                name: "unsafe_fn2".to_string(),
                                type_args: vec![],
                                args: vec![],
                            }),
                        },
                    ],
                    result: Some(Box::new(Expr::EVoid { loc: loc() })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.iter().any(|e| e.code == "E0009" && e.message.contains("unsafe function")));
    }

    #[test]
    fn test_nested_block_scoping() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SLet {
                            loc: loc(),
                            mutable: false,
                            name: "x".to_string(),
                            expr: Box::new(Expr::EInt { loc: loc(), value: 1 }),
                        },
                        Stmt::SExpr {
                            loc: loc(),
                            expr: Box::new(Expr::EBlock {
                                loc: loc(),
                                stmts: vec![Stmt::SLet {
                                    loc: loc(),
                                    mutable: false,
                                    name: "x".to_string(),
                                    expr: Box::new(Expr::EInt { loc: loc(), value: 2 }),
                                }],
                                result: Some(Box::new(Expr::EVar {
                                    loc: loc(),
                                    name: "x".to_string(),
                                })),
                            }),
                        },
                    ],
                    result: Some(Box::new(Expr::EVar {
                        loc: loc(),
                        name: "x".to_string(),
                    })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.is_empty(), "nested block should shadow outer var, got: {:?}", errs);
    }

    #[test]
    fn test_move_in_different_branches() {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![Param::POwn {
                    name: "x".to_string(),
                    typ: Typ::TInt,
                }],
                Expr::EIf {
                    loc: loc(),
                    cond: Box::new(Expr::EBool { loc: loc(), value: true }),
                    then: Box::new(Expr::EMove {
                        loc: loc(),
                        name: "x".to_string(),
                    }),
                    else_: Some(Box::new(Expr::EMove {
                        loc: loc(),
                        name: "x".to_string(),
                    })),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.is_empty(), "move in both branches should be ok, got: {:?}", errs);
    }

    #[test]
    fn test_struct_field_access_valid() {
        let defs = vec![
            make_module("test"),
            make_struct(
                "Point",
                vec![
                    FieldDef { name: "x".to_string(), typ: Typ::TInt },
                    FieldDef { name: "y".to_string(), typ: Typ::TInt },
                ],
            ),
            make_func(
                "main",
                vec![],
                Expr::EFieldAccess {
                    loc: loc(),
                    expr: Box::new(Expr::EStructLit {
                        loc: loc(),
                        name: "Point".to_string(),
                        fields: vec![
                            ValueField {
                                name: "x".to_string(),
                                value: Expr::EInt { loc: loc(), value: 1 },
                            },
                            ValueField {
                                name: "y".to_string(),
                                value: Expr::EInt { loc: loc(), value: 2 },
                            },
                        ],
                        type_args: vec![],
                    }),
                    field: "x".to_string(),
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.is_empty(), "valid field access should have no errors, got: {:?}", errs);
    }

    // ── droppable move-only enforcement ─────────────────────────────

    fn file_typ() -> Typ {
        Typ::TStruct {
            name: "File".to_string(),
            fields: vec![],
            type_args: vec![],
        }
    }

    fn file_lit() -> Expr {
        Expr::EStructLit {
            loc: loc(),
            name: "File".to_string(),
            fields: vec![],
            type_args: vec![],
        }
    }

    fn drop_defs() -> Vec<Def> {
        vec![
            make_module("test"),
            make_struct("File", vec![FieldDef { name: "id".to_string(), typ: Typ::TInt }]),
            Def::DImpl {
                loc: loc(),
                struct_name: "File".to_string(),
                impls: vec![ImplExpr {
                    op: ImplOp::ImDrop,
                    func: "file_close".to_string(),
                    loc: loc(),
                }],
            },
            make_func(
                "file_close",
                vec![Param::PRef { name: "self".to_string(), typ: file_typ() }],
                Expr::EVoid { loc: loc() },
                Safety::Safe,
            ),
        ]
    }

    fn let_file(name: &str) -> Stmt {
        Stmt::SLetTyped {
            loc: loc(),
            name: name.to_string(),
            typ: file_typ(),
            expr: Box::new(file_lit()),
        }
    }

    fn use_var(name: &str) -> Stmt {
        Stmt::SExpr {
            loc: loc(),
            expr: Box::new(Expr::EVar {
                loc: loc(),
                name: name.to_string(),
            }),
        }
    }

    #[test]
    fn test_droppable_second_use_after_implicit_move_errors() {
        let mut defs = drop_defs();
        defs.push(make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    let_file("a"),
                    Stmt::SLetTyped {
                        loc: loc(),
                        name: "b".to_string(),
                        typ: file_typ(),
                        expr: Box::new(Expr::EVar { loc: loc(), name: "a".to_string() }),
                    },
                    use_var("a"),
                ],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(errs.iter().any(|e| e.code == "E0001"), "expected E0001, got: {:?}", errs);
    }

    #[test]
    fn test_droppable_clone_allowed() {
        let mut defs = drop_defs();
        defs.push(make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    let_file("a"),
                    Stmt::SLetTyped {
                        loc: loc(),
                        name: "b".to_string(),
                        typ: file_typ(),
                        expr: Box::new(Expr::EClone { loc: loc(), name: "a".to_string() }),
                    },
                    use_var("a"),
                ],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(errs.is_empty(), "clone of droppable should be allowed, got: {:?}", errs);
    }

    #[test]
    fn test_droppable_call_arg_moves_ref_arg_does_not() {
        let mut defs = drop_defs();
        defs.push(make_func(
            "consume",
            vec![Param::POwn { name: "f".to_string(), typ: file_typ() }],
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ));
        defs.push(make_func(
            "inspect",
            vec![Param::PRef { name: "f".to_string(), typ: file_typ() }],
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ));
        // ref arg does not move: inspect(a); use a → ok
        defs.push(make_func(
            "ok_fn",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    let_file("a"),
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::ECall {
                            loc: loc(),
                            name: "inspect".to_string(),
                            type_args: vec![],
                            args: vec![Expr::EVar { loc: loc(), name: "a".to_string() }],
                        }),
                    },
                    use_var("a"),
                ],
                result: None,
            },
            Safety::Safe,
        ));
        // own arg moves: consume(b); use b → E0001
        defs.push(make_func(
            "bad_fn",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    let_file("b"),
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::ECall {
                            loc: loc(),
                            name: "consume".to_string(),
                            type_args: vec![],
                            args: vec![Expr::EVar { loc: loc(), name: "b".to_string() }],
                        }),
                    },
                    use_var("b"),
                ],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(errs.iter().any(|e| e.code == "E0001"), "expected E0001 for own-arg reuse, got: {:?}", errs);
        assert_eq!(errs.iter().filter(|e| e.code == "E0001").count(), 1, "ref arg must not move, got: {:?}", errs);
    }

    #[test]
    fn test_droppable_branch_inconsistent_move_errors() {
        let mut defs = drop_defs();
        defs.push(make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    let_file("a"),
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EIf {
                            loc: loc(),
                            cond: Box::new(Expr::EBool { loc: loc(), value: true }),
                            then: Box::new(Expr::EBlock {
                                loc: loc(),
                                stmts: vec![Stmt::SExpr {
                                    loc: loc(),
                                    expr: Box::new(Expr::EMove { loc: loc(), name: "a".to_string() }),
                                }],
                                result: None,
                            }),
                            else_: None,
                        }),
                    },
                ],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(errs.iter().any(|e| e.code == "E0033"), "expected E0033, got: {:?}", errs);
    }

    #[test]
    fn test_droppable_both_branches_move_ok() {
        let move_a_block = Expr::EBlock {
            loc: loc(),
            stmts: vec![Stmt::SExpr {
                loc: loc(),
                expr: Box::new(Expr::EMove { loc: loc(), name: "a".to_string() }),
            }],
            result: None,
        };
        let mut defs = drop_defs();
        defs.push(make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    let_file("a"),
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EIf {
                            loc: loc(),
                            cond: Box::new(Expr::EBool { loc: loc(), value: true }),
                            then: Box::new(move_a_block.clone()),
                            else_: Some(Box::new(move_a_block)),
                        }),
                    },
                ],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(!errs.iter().any(|e| e.code == "E0033"), "both branches move is legal, got: {:?}", errs);
    }

    #[test]
    fn test_non_droppable_struct_second_use_still_allowed() {
        let defs = vec![
            make_module("test"),
            make_struct("Point", vec![FieldDef { name: "x".to_string(), typ: Typ::TInt }]),
            make_func(
                "main",
                vec![],
                Expr::EBlock {
                    loc: loc(),
                    stmts: vec![
                        Stmt::SLetTyped {
                            loc: loc(),
                            name: "p".to_string(),
                            typ: Typ::TStruct {
                                name: "Point".to_string(),
                                fields: vec![],
                                type_args: vec![],
                            },
                            expr: Box::new(Expr::EStructLit {
                                loc: loc(),
                                name: "Point".to_string(),
                                fields: vec![ValueField {
                                    name: "x".to_string(),
                                    value: Expr::EInt { loc: loc(), value: 1 },
                                }],
                                type_args: vec![],
                            }),
                        },
                        Stmt::SLetTyped {
                            loc: loc(),
                            name: "q".to_string(),
                            typ: Typ::TStruct {
                                name: "Point".to_string(),
                                fields: vec![],
                                type_args: vec![],
                            },
                            expr: Box::new(Expr::EVar { loc: loc(), name: "p".to_string() }),
                        },
                        use_var("p"),
                    ],
                    result: None,
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        assert!(errs.is_empty(), "non-droppable struct copy must stay legal, got: {:?}", errs);
    }

    // ── builtin drop(x) ─────────────────────────────────────────────

    fn drop_call(var: &str) -> Stmt {
        Stmt::SExpr {
            loc: loc(),
            expr: Box::new(Expr::ECall {
                loc: loc(),
                name: "drop".to_string(),
                type_args: vec![],
                args: vec![Expr::EVar { loc: loc(), name: var.to_string() }],
            }),
        }
    }

    #[test]
    fn test_drop_builtin_marks_var_moved() {
        let mut defs = drop_defs();
        defs.push(make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![let_file("a"), drop_call("a"), use_var("a")],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(errs.iter().any(|e| e.code == "E0001"), "use after drop(a) should be E0001, got: {:?}", errs);
    }

    #[test]
    fn test_drop_builtin_valid_use_no_errors() {
        let mut defs = drop_defs();
        defs.push(make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![let_file("a"), drop_call("a")],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(errs.is_empty(), "valid drop(a) should have no errors, got: {:?}", errs);
    }

    #[test]
    fn test_drop_builtin_on_non_droppable_is_e0035() {
        let mut defs = drop_defs();
        defs.push(make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SLet {
                        loc: loc(),
                        mutable: false,
                        name: "n".to_string(),
                        expr: Box::new(Expr::EInt { loc: loc(), value: 1 }),
                    },
                    drop_call("n"),
                ],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(errs.iter().any(|e| e.code == "E0035"), "drop on non-droppable should be E0035, got: {:?}", errs);
    }

    #[test]
    fn test_drop_builtin_on_complex_expr_is_e0035() {
        let mut defs = drop_defs();
        defs.push(make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SExpr {
                    loc: loc(),
                    expr: Box::new(Expr::ECall {
                        loc: loc(),
                        name: "drop".to_string(),
                        type_args: vec![],
                        args: vec![file_lit()],
                    }),
                }],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(errs.iter().any(|e| e.code == "E0035"), "drop on non-variable should be E0035, got: {:?}", errs);
    }

    #[test]
    fn test_drop_in_one_branch_move_in_other_is_consistent() {
        let mut defs = drop_defs();
        defs.push(make_func(
            "consume",
            vec![Param::POwn { name: "f".to_string(), typ: file_typ() }],
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ));
        defs.push(make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    let_file("a"),
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EIf {
                            loc: loc(),
                            cond: Box::new(Expr::EBool { loc: loc(), value: true }),
                            then: Box::new(Expr::EBlock {
                                loc: loc(),
                                stmts: vec![drop_call("a")],
                                result: None,
                            }),
                            else_: Some(Box::new(Expr::EBlock {
                                loc: loc(),
                                stmts: vec![Stmt::SExpr {
                                    loc: loc(),
                                    expr: Box::new(Expr::ECall {
                                        loc: loc(),
                                        name: "consume".to_string(),
                                        type_args: vec![],
                                        args: vec![Expr::EMove { loc: loc(), name: "a".to_string() }],
                                    }),
                                }],
                                result: None,
                            })),
                        }),
                    },
                ],
                result: None,
            },
            Safety::Safe,
        ));
        let errs = check_program(&defs);
        assert!(!errs.iter().any(|e| e.code == "E0033"), "drop in one branch + move in other should be consistent, got: {:?}", errs);
    }
}
