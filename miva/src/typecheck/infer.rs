use super::*;
use crate::ast::*;
use crate::error::Error;
use std::collections::HashMap;

pub(crate) fn infer_type(
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
    match e {
        Expr::EInt { .. } => (Typ::TInt, vec![]),
        Expr::EBool { .. } => (Typ::TBool, vec![]),
        Expr::EFloat { .. } => (Typ::TFloat64, vec![]),
        Expr::EChar { .. } => (Typ::TChar, vec![]),
        Expr::EString { .. } => (Typ::TString, vec![]),
        Expr::EVoid { .. } => (Typ::TNull, vec![]),
        Expr::EVar { name, .. } => match env.vars.get(name) {
            Some(t) => (t.clone(), vec![]),
            None => (Typ::TInvalid, vec![]),
        },
        Expr::EMove { name, .. } => match env.vars.get(name) {
            Some(t) => (t.clone(), vec![]),
            None => (Typ::TInvalid, vec![]),
        },
        Expr::EClone { name, .. } => match env.vars.get(name) {
            Some(t) => (t.clone(), vec![]),
            None => (Typ::TInvalid, vec![]),
        },
        Expr::EAddr { expr, loc, .. } => {
            let (inner_typ, mut errs) = infer_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                expr,
            );
            if matches!(inner_typ, Typ::TNull | Typ::TInvalid) {
                errs.push(Error::new(
                    "E0014",
                    loc,
                    "cannot take address of expression with no value",
                ));
            }
            (
                Typ::TPtr {
                    to: Box::new(inner_typ),
                },
                errs,
            )
        }
        Expr::EDeref { expr, loc, .. } => {
            let (inner_typ, mut errs) = infer_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                expr,
            );
            match inner_typ {
                Typ::TPtr { to } => (*to, errs),
                _ => {
                    errs.push(Error::new(
                        "E0014",
                        loc,
                        &format!("cannot dereference non-pointer type"),
                    ));
                    (Typ::TInvalid, errs)
                }
            }
        }
        Expr::EBinOp {
            op,
            left,
            right,
            loc,
        } => {
            let (lt, mut errs) = require_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                left,
            );
            let (rt, errs2) = require_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                right,
            );
            errs.extend(errs2);

            match op {
                BinOp::Add => {
                    // Allow numeric + numeric (arithmetic) and string + string (concatenation)
                    if lt == Typ::TString && rt == Typ::TString {
                        (Typ::TString, errs)
                    } else if is_numeric(&lt) && is_numeric(&rt) {
                        if !types_equal(&lt, &rt) {
                            errs.push(Error::new(
                                "E0014",
                                loc,
                                &format!("type mismatch in arithmetic: {:?} vs {:?}", lt, rt),
                            ));
                            (Typ::TInvalid, errs)
                        } else {
                            (lt, errs)
                        }
                    } else {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            &format!(
                                "addition requires numeric or string types, got {:?} and {:?}",
                                lt, rt
                            ),
                        ));
                        (Typ::TInvalid, errs)
                    }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    if !is_numeric(&lt) || !is_numeric(&rt) {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            &format!(
                                "arithmetic operation requires numeric types, got {:?} and {:?}",
                                lt, rt
                            ),
                        ));
                        (Typ::TInvalid, errs)
                    } else if !types_equal(&lt, &rt) {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            &format!("type mismatch in arithmetic: {:?} vs {:?}", lt, rt),
                        ));
                        (Typ::TInvalid, errs)
                    } else {
                        (lt, errs)
                    }
                }
                BinOp::Eq | BinOp::Neq => {
                    if !types_equal(&lt, &rt) {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            &format!("type mismatch in comparison: {:?} vs {:?}", lt, rt),
                        ));
                        (Typ::TInvalid, errs)
                    } else {
                        (Typ::TBool, errs)
                    }
                }
                BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    // Ordering comparisons: both sides must be numeric (and
                    // equal-typed) so the C++ backend can emit `<`/`>`/`<=`/`>=`.
                    if !is_numeric(&lt) || !is_numeric(&rt) {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            &format!(
                                "ordering comparison requires numeric types, got {:?} and {:?}",
                                lt, rt
                            ),
                        ));
                        (Typ::TInvalid, errs)
                    } else if !types_equal(&lt, &rt) {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            &format!("type mismatch in comparison: {:?} vs {:?}", lt, rt),
                        ));
                        (Typ::TInvalid, errs)
                    } else {
                        (Typ::TBool, errs)
                    }
                }
                BinOp::And | BinOp::Or => {
                    // Logical AND/OR: both sides must be bool. Short-circuit
                    // semantics are emitted by the C++ backend (`&&`/`||`).
                    if lt != Typ::TBool || rt != Typ::TBool {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            &format!(
                                "logical operator requires bool operands, got {:?} and {:?}",
                                lt, rt
                            ),
                        ));
                        (Typ::TInvalid, errs)
                    } else {
                        (Typ::TBool, errs)
                    }
                }
            }
        }
        Expr::EIf {
            cond,
            then,
            else_,
            loc,
        } => {
            let (ct, mut errs) = require_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                cond,
            );
            if !types_equal(&ct, &Typ::TBool) {
                errs.push(Error::new(
                    "E0014",
                    loc,
                    &format!("if condition must be bool"),
                ));
            }

            let (tt, errs2) = infer_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                then,
            );
            errs.extend(errs2);

            match else_ {
                Some(else_expr) => {
                    let (et, errs3) = infer_type(
                        env,
                        func_sigs,
                        structs,
                        struct_type_params,
                        enums,
                        enum_type_params,
                        func_return,
                        droppable,
                        else_expr,
                    );
                    errs.extend(errs3);
                    if !types_equal(&tt, &et) {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            &format!("if and else branches have different types",),
                        ));
                    }
                    (tt, errs)
                }
                None => (Typ::TNull, errs),
            }
        }
        Expr::EChoose {
            var,
            cases,
            otherwise,
            loc,
        } => {
            let (vt, mut errs) = require_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                var,
            );
            let _ = vt;
            let mut tmp_typ = vec![];

            if let Some(else_expr) = otherwise {
                let (et, errs3) = infer_type(
                    env,
                    func_sigs,
                    structs,
                    struct_type_params,
                    enums,
                    enum_type_params,
                    func_return,
                    droppable,
                    else_expr,
                );
                errs.extend(errs3);

                let mut all_same = true;
                let mut first_type: Option<Typ> = Some(et);
                let mut first_case = true;
                for case in cases {
                    let (wt, errs_w) = require_type(
                        env,
                        func_sigs,
                        structs,
                        struct_type_params,
                        enums,
                        enum_type_params,
                        func_return,
                        droppable,
                        &case.when,
                    );
                    errs.extend(errs_w);
                    // Allow enum type comparison even when type_args differ
                    let is_enum_match = match (&wt, &vt) {
                        (Typ::TStruct { name: n1, .. }, Typ::TStruct { name: n2, .. }) => {
                            n1 == n2 && enums.contains_key(n1)
                        }
                        _ => false,
                    };
                    if !is_enum_match && !types_equal(&wt, &vt) {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            "choose expr and the variable must have the same type",
                        ));
                    }
                    // Register enum-pattern destructuring bindings so the `then`
                    // branch can refer to them with the variant's payload types.
                    let mut saved_bindings: Vec<(String, Option<Typ>)> = Vec::new();
                    if let Expr::EEnumPattern {
                        enum_name,
                        variant,
                        bindings,
                        ..
                    } = case.when.as_ref()
                    {
                        if let Some(variants) = enums.get(enum_name.as_str()) {
                            if let Some(v) = variants.iter().find(|v| &v.name == variant) {
                                let enum_tp = enum_type_params
                                    .get(enum_name.as_str())
                                    .cloned()
                                    .unwrap_or_default();
                                let vt_type_args = match &vt {
                                    Typ::TStruct { type_args, .. } => type_args.clone(),
                                    _ => vec![],
                                };
                                let mut subst = HashMap::new();
                                for (tp, ta) in enum_tp.iter().zip(vt_type_args.iter()) {
                                    subst.insert(tp.clone(), ta.clone());
                                }
                                for (b, bt) in bindings.iter().zip(v.payload.iter()) {
                                    let resolved = if !subst.is_empty() {
                                        resolve_type(&normalize_typ(bt, &enum_tp), &subst)
                                    } else {
                                        bt.clone()
                                    };
                                    saved_bindings
                                        .push((b.clone(), env.vars.insert(b.clone(), resolved)));
                                }
                            }
                        }
                    }
                    let (tt, errs_t) = infer_type(
                        env,
                        func_sigs,
                        structs,
                        struct_type_params,
                        enums,
                        enum_type_params,
                        func_return,
                        droppable,
                        &case.then,
                    );
                    errs.extend(errs_t);
                    for (b, prev) in saved_bindings {
                        match prev {
                            Some(p) => {
                                env.vars.insert(b, p);
                            }
                            None => {
                                env.vars.remove(&b);
                            }
                        }
                    }
                    if first_case {
                        first_type = Some(tt);
                        first_case = false;
                    } else if let Some(ref ft) = first_type {
                        if !types_equal(&tt, ft) {
                            all_same = false;
                        }
                    }
                }

                if !all_same {
                    errs.push(Error::new(
                        "E0014",
                        loc,
                        "choose branches must all have the same type",
                    ));
                }

                if let Some(ref ft) = first_type {
                    (ft.clone(), errs)
                } else {
                    (Typ::TNull, errs)
                }
            } else {
                for case in cases {
                    let (wt, errs_w) = require_type(
                        env,
                        func_sigs,
                        structs,
                        struct_type_params,
                        enums,
                        enum_type_params,
                        func_return,
                        droppable,
                        &case.when,
                    );
                    errs.extend(errs_w);
                    if !types_equal(&wt, &vt) {
                        errs.push(Error::new(
                            "E0014",
                            loc,
                            "choose expr and the variable must have the same type",
                        ));
                    }
                    let (tt, errs_t) = infer_type(
                        env,
                        func_sigs,
                        structs,
                        struct_type_params,
                        enums,
                        enum_type_params,
                        func_return,
                        droppable,
                        &case.then,
                    );
                    errs.extend(errs_t);
                    if tmp_typ.is_empty() && tt != Typ::TInvalid {
                        tmp_typ.push(tt);
                    } else {
                        let t1 = tmp_typ.get(0).unwrap_or(&Typ::TInvalid);
                        if t1 == &Typ::TInvalid || tt == Typ::TInvalid {
                            errs.push(Error::new(
                                "E0014",
                                loc,
                                "choose branches must not have invalid type",
                            ));
                        }
                        if !types_equal(&tt, t1) {
                            errs.push(Error::new(
                                "E0014",
                                loc,
                                "choose branches must all have the same type",
                            ));
                        }
                    }
                }
                (tmp_typ.get(0).unwrap_or(&Typ::TInvalid).clone(), errs)
            }
        }
        Expr::EEnumPattern {
            enum_name,
            variant,
            bindings,
            loc,
        } => {
            // Enum destructuring pattern used in `when (Enum.Variant(x, y))`.
            // Validate the enum and variant, and that the number of bindings
            // matches the variant's payload arity.
            let mut errs = vec![];
            let typ = match enums.get(enum_name.as_str()) {
                Some(variants) => match variants.iter().find(|v| &v.name == variant) {
                    Some(v) => {
                        if v.payload.len() != bindings.len() && !bindings.is_empty() {
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
                        Typ::TStruct {
                            name: enum_name.clone(),
                            fields: vec![],
                            type_args: vec![],
                        }
                    }
                    None => {
                        errs.push(Error::new(
                            "E0019",
                            loc,
                            &format!("unknown variant '{}' in enum '{}'", variant, enum_name),
                        ));
                        Typ::TInvalid
                    }
                },
                None => {
                    errs.push(Error::new(
                        "E0018",
                        loc,
                        &format!("unknown enum '{}'", enum_name),
                    ));
                    Typ::TInvalid
                }
            };
            (typ, errs)
        }
        Expr::ELambda {
            params,
            ret,
            body,
            loc,
            ..
        } => {
            let mut errs = vec![];
            // Build a child environment with the lambda's own params.
            let mut child_env = env.clone();
            for p in params {
                let (name, typ) = match p {
                    Param::PRef { name, typ } => (name.clone(), typ.clone()),
                    Param::POwn { name, typ } => (name.clone(), typ.clone()),
                };
                child_env.vars.insert(name, typ);
            }
            let (body_typ, body_errs) = infer_type(
                &mut child_env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                &Some(ret.clone()),
                droppable,
                body,
            );
            errs.extend(body_errs);
            if !matches!(body_typ, Typ::TNull) && !types_equal(&body_typ, ret) {
                errs.push(Error::new(
                    "E0020",
                    loc,
                    &format!(
                        "lambda body type {:?} does not match declared return {:?}",
                        body_typ, ret
                    ),
                ));
            }
            let param_typs: Vec<Typ> = params
                .iter()
                .map(|p| match p {
                    Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ.clone(),
                })
                .collect();
            (
                Typ::TFunc {
                    params: param_typs,
                    returns: Box::new(ret.clone()),
                },
                errs,
            )
        }
        Expr::ECall {
            name,
            type_args,
            args,
            loc,
        } => {
            // Build substitution from explicit or inferred type args for generic enums.
            // `loc` and `enum_name` are used only for error reporting.
            fn resolve_enum_type_args(
                enum_name: &str,
                enum_type_params: &[String],
                call_type_args: &[Typ],
                payload: &[Typ],
                args: &[Expr],
                loc: &crate::ast::Loc,
                env: &mut TypeEnv,
                func_sigs: &HashMap<String, (Vec<String>, Vec<Param>, Option<Typ>)>,
                structs: &HashMap<String, Vec<FieldDef>>,
                struct_type_params: &HashMap<String, Vec<String>>,
                enums: &HashMap<String, Vec<crate::ast::EnumVariant>>,
                enum_type_params_map: &HashMap<String, Vec<String>>,
                func_return: &Option<Typ>,
                droppable: &HashSet<String>,
            ) -> (Vec<Typ>, HashMap<String, Typ>, Vec<Error>) {
                let mut errs = vec![];
                if enum_type_params.is_empty() {
                    return (vec![], HashMap::new(), errs);
                }
                let subst: HashMap<String, Typ>;
                let resolved: Vec<Typ>;
                if !call_type_args.is_empty() {
                    if call_type_args.len() != enum_type_params.len() {
                        errs.push(Error::new(
                            "E0016",
                            loc,
                            &format!(
                                "enum '{}' takes {} type argument(s), got {}",
                                enum_name,
                                enum_type_params.len(),
                                call_type_args.len()
                            ),
                        ));
                        resolved = enum_type_params.iter().map(|_| Typ::TInvalid).collect();
                        subst = HashMap::new();
                    } else {
                        subst = enum_type_params
                            .iter()
                            .cloned()
                            .zip(call_type_args.iter().cloned())
                            .collect();
                        resolved = call_type_args.to_vec();
                    }
                } else {
                    let mut inferred = HashMap::new();
                    for (pt, a) in payload.iter().zip(args.iter()) {
                        let pt_norm = normalize_typ(pt, enum_type_params);
                        let (at, _) = require_type(
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params_map,
                            func_return,
                            droppable,
                            a,
                        );
                        infer_type_from_arg(&pt_norm, &at, &mut inferred);
                    }
                    for tp in enum_type_params {
                        if !inferred.contains_key(tp) {
                            inferred.insert(tp.clone(), Typ::TInvalid);
                        }
                    }
                    resolved = enum_type_params
                        .iter()
                        .map(|tp| inferred.get(tp).cloned().unwrap_or(Typ::TInvalid))
                        .collect();
                    // v1 generic-argument ban (E0036) for *inferred* type
                    // args; explicit type args are checked in semantic.
                    for t in inferred.values() {
                        if crate::droppable::is_droppable_typ(droppable, t) {
                            errs.push(Error::new(
                                "E0036",
                                loc,
                                &format!(
                                    "droppable type '{}' cannot be used as a generic argument in v1 (inferred for enum '{}')",
                                    crate::droppable::droppable_typ_name(t),
                                    enum_name
                                ),
                            ));
                        }
                    }
                    subst = inferred;
                }
                (resolved, subst, errs)
            }

            // Enum constructor in dotted-name form: `Name.Variant(args)`.
            if let Some(dot) = name.find('.') {
                let enum_name = &name[..dot];
                let variant_name = &name[dot + 1..];
                if let Some(variants) = enums.get(enum_name) {
                    if let Some(v) = variants.iter().find(|v| v.name == variant_name) {
                        let mut errs = Vec::new();
                        let enum_tp = enum_type_params.get(enum_name).cloned().unwrap_or_default();
                        if v.payload.len() != args.len() {
                            errs.push(Error::new(
                                "E0016",
                                loc,
                                &format!(
                                    "enum variant '{}' expects {} argument(s), got {}",
                                    variant_name,
                                    v.payload.len(),
                                    args.len()
                                ),
                            ));
                        }
                        let (resolved_type_args, subst, subst_errs) = resolve_enum_type_args(
                            enum_name,
                            &enum_tp,
                            type_args,
                            &v.payload,
                            args,
                            loc,
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                        );
                        errs.extend(subst_errs);
                        for (i, (pt, a)) in v.payload.iter().zip(args.iter()).enumerate() {
                            let pt_resolved = if !subst.is_empty() {
                                resolve_type(&normalize_typ(pt, &enum_tp), &subst)
                            } else {
                                pt.clone()
                            };
                            let (at, ae) = require_type(
                                env,
                                func_sigs,
                                structs,
                                struct_type_params,
                                enums,
                                enum_type_params,
                                func_return,
                                droppable,
                                a,
                            );
                            errs.extend(ae);
                            if !types_equal(&pt_resolved, &at) {
                                errs.push(Error::new("E0014", loc, &format!(
                                    "enum variant '{}' argument {} type mismatch: expected {:?}, got {:?}",
                                    variant_name, i, pt_resolved, at)));
                            }
                        }
                        return (
                            Typ::TStruct {
                                name: enum_name.to_string(),
                                fields: vec![],
                                type_args: resolved_type_args,
                            },
                            errs,
                        );
                    }
                }
            } else if let Some(enum_name) = args.first().and_then(|a| match a {
                Expr::EVar { name: n, .. } => Some(n.clone()),
                _ => None,
            }) {
                // Enum constructor in desugared method-call form: `Variant(EnumName, payload...)`.
                // The frontend parses `Shape.Circle(5)` as EMethodCall and macro_expand
                // desugars it to `ECall { name: "Circle", args: [EVar("Shape"), EInt(5)] }`.
                if let Some(variants) = enums.get(&enum_name) {
                    if let Some(v) = variants.iter().find(|v| v.name == *name) {
                        let payload_args = &args[1..];
                        let mut errs = Vec::new();
                        let enum_tp = enum_type_params
                            .get(&enum_name)
                            .cloned()
                            .unwrap_or_default();
                        if v.payload.len() != payload_args.len() {
                            errs.push(Error::new(
                                "E0016",
                                loc,
                                &format!(
                                    "enum variant '{}' expects {} argument(s), got {}",
                                    name,
                                    v.payload.len(),
                                    payload_args.len()
                                ),
                            ));
                        }
                        let (resolved_type_args, subst, subst_errs) = resolve_enum_type_args(
                            &enum_name,
                            &enum_tp,
                            type_args,
                            &v.payload,
                            payload_args,
                            loc,
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                        );
                        errs.extend(subst_errs);
                        for (i, (pt, a)) in v.payload.iter().zip(payload_args.iter()).enumerate() {
                            let pt_resolved = if !subst.is_empty() {
                                resolve_type(&normalize_typ(pt, &enum_tp), &subst)
                            } else {
                                pt.clone()
                            };
                            let (at, ae) = require_type(
                                env,
                                func_sigs,
                                structs,
                                struct_type_params,
                                enums,
                                enum_type_params,
                                func_return,
                                droppable,
                                a,
                            );
                            errs.extend(ae);
                            if !types_equal(&pt_resolved, &at) {
                                errs.push(Error::new("E0014", loc, &format!(
                                    "enum variant '{}' argument {} type mismatch: expected {:?}, got {:?}",
                                    name, i, pt_resolved, at)));
                            }
                        }
                        return (
                            Typ::TStruct {
                                name: enum_name.to_string(),
                                fields: vec![],
                                type_args: resolved_type_args,
                            },
                            errs,
                        );
                    }
                }
            }
            let mut errs = vec![];
            let mut arg_types = vec![];
            for arg in args {
                let (at, ae) = require_type(
                    env,
                    func_sigs,
                    structs,
                    struct_type_params,
                    enums,
                    enum_type_params,
                    func_return,
                    droppable,
                    arg,
                );
                arg_types.push(at);
                errs.extend(ae);
            }

            let user_func = func_sigs.get(name.as_str());
            let ret_typ = if let Some((fn_type_params, params, ret)) = user_func {
                // Build type substitution: generic param -> concrete type
                let mut type_subst: HashMap<String, Typ> = HashMap::new();

                if params.len() != args.len() {
                    errs.push(Error::new(
                        "E0016",
                        loc,
                        &format!(
                            "function '{}' takes {} arguments but got {}",
                            name,
                            params.len(),
                            args.len()
                        ),
                    ));
                } else {
                    // Apply explicit type args if provided
                    if !type_args.is_empty() {
                        if type_args.len() != fn_type_params.len() {
                            errs.push(Error::new(
                                "E0016",
                                loc,
                                &format!(
                                    "function '{}' takes {} type arguments but got {}",
                                    name,
                                    fn_type_params.len(),
                                    type_args.len()
                                ),
                            ));
                        } else {
                            // fn_type_params: the callee's generic param names.
                            // type_args: caller-supplied types (e.g. `push[T]`→`T`,
                            // `push[int]`→`int`). A bare `TStruct{name:"T"}`
                            // type arg that names an *outer* generic param must be
                            // stored as `TGenericParam` so resolve_type can chain
                            // substitutions correctly (else `Vec[T]` resolves to
                            // `Vec<TStruct{name:"T"}>` which won't equal a real
                            // `Vec<TGenericParam{name:"T"}>` arg type).
                            for (tp, ta) in fn_type_params.iter().zip(type_args.iter()) {
                                let norm_ta = match ta {
                                    Typ::TStruct {
                                        name,
                                        fields,
                                        type_args,
                                    } if fields.is_empty()
                                        && type_args.is_empty()
                                        && name != "Vec" =>
                                    {
                                        Typ::TGenericParam { name: name.clone() }
                                    }
                                    _ => ta.clone(),
                                };
                                type_subst.insert(tp.clone(), norm_ta);
                            }
                        }
                    }

                    // Infer remaining or all type params from args
                    if !fn_type_params.is_empty() {
                        for (idx, (param, arg_t)) in params.iter().zip(arg_types.iter()).enumerate()
                        {
                            let param_t = match param {
                                Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ,
                            };
                            if !type_args.is_empty() {
                                // With explicit type args, check compatibility
                                let resolved = resolve_type(param_t, &type_subst);
                                if !types_equal(&resolved, arg_t) {
                                    errs.push(Error::new(
                                        "E0016",
                                        loc,
                                        &format!(
                                            "argument {} to function '{}' has wrong type",
                                            idx + 1,
                                            name
                                        ),
                                    ));
                                }
                            } else {
                                // Infer type params from argument
                                infer_type_from_arg(param_t, arg_t, &mut type_subst);
                            }
                        }
                        // v1 generic-argument ban (E0036) for *inferred* type
                        // args; explicit type args are checked in semantic.
                        if type_args.is_empty() {
                            for t in type_subst.values() {
                                if crate::droppable::is_droppable_typ(droppable, t) {
                                    errs.push(Error::new(
                                        "E0036",
                                        loc,
                                        &format!(
                                            "droppable type '{}' cannot be used as a generic argument in v1 (inferred in call to '{}')",
                                            crate::droppable::droppable_typ_name(t),
                                            name
                                        ),
                                    ));
                                }
                            }
                        }
                        // Check for type consistency (all args must agree)
                        for (idx, (param, arg_t)) in params.iter().zip(arg_types.iter()).enumerate()
                        {
                            let param_t = match param {
                                Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ,
                            };
                            let resolved = resolve_type(param_t, &type_subst);
                            if !types_equal(&resolved, arg_t) {
                                errs.push(Error::new(
                                    "E0016",
                                    loc,
                                    &format!(
                                        "argument {} to function '{}' has wrong type",
                                        idx + 1,
                                        name
                                    ),
                                ));
                            }
                        }
                    } else {
                        // Non-generic function: check types directly
                        for (i, (param, arg_t)) in params.iter().zip(arg_types.iter()).enumerate() {
                            let param_t = match param {
                                Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ,
                            };
                            if !types_equal(param_t, arg_t) {
                                errs.push(Error::new(
                                    "E0016",
                                    loc,
                                    &format!(
                                        "argument {} to function '{}' has wrong type",
                                        i + 1,
                                        name
                                    ),
                                ));
                            }
                        }
                    }
                }
                // Resolve return type using substitution
                ret.as_ref()
                    .map(|rt| resolve_type(rt, &type_subst))
                    .unwrap_or(Typ::TNull)
            } else {
                match name.as_str() {
                    "box_new" => {
                        if args.is_empty() {
                            Typ::TNull
                        } else {
                            Typ::TBox {
                                of: Box::new(arg_types[0].clone()),
                            }
                        }
                    }
                    "box_deref" => {
                        if args.is_empty() {
                            Typ::TNull
                        } else {
                            match &arg_types[0] {
                                Typ::TBox { of } => *of.clone(),
                                _ => Typ::TInvalid,
                            }
                        }
                    }
                    "await" => {
                        if args.is_empty() {
                            Typ::TNull
                        } else {
                            match &arg_types[0] {
                                Typ::TFuture { of } => (**of).clone(),
                                _ => Typ::TInvalid,
                            }
                        }
                    }
                    name => {
                        // Check if this is a closure variable call
                        if let Some(Typ::TFunc {
                            params: fn_pt,
                            returns: fn_ret,
                        }) = env.vars.get(name)
                        {
                            if fn_pt.len() != arg_types.len() {
                                errs.push(Error::new(
                                    "E0016",
                                    loc,
                                    &format!(
                                        "closure '{}' takes {} arguments but got {}",
                                        name,
                                        fn_pt.len(),
                                        arg_types.len()
                                    ),
                                ));
                            }
                            for (i, (pt, at)) in fn_pt.iter().zip(arg_types.iter()).enumerate() {
                                if !types_equal(pt, at) {
                                    errs.push(Error::new(
                                        "E0016",
                                        loc,
                                        &format!("argument {} to closure '{}' has wrong type: expected {:?}, got {:?}", i + 1, name, pt, at),
                                    ));
                                }
                            }
                            *fn_ret.clone()
                        } else {
                            builtin_return_typ(name).unwrap_or(Typ::TNull)
                        }
                    }
                }
            };
            (ret_typ, errs)
        }
        Expr::EStructLit {
            name,
            fields,
            type_args,
            loc,
        } => {
            let mut errs = vec![];
            let struct_fields = match structs.get(name.as_str()) {
                Some(f) => f,
                None => {
                    errs.push(Error::new(
                        "E0018",
                        loc,
                        &format!("unknown struct '{}'", name),
                    ));
                    return (Typ::TInvalid, errs);
                }
            };

            let mut field_map: HashMap<&str, &Typ> = HashMap::new();
            for sf in struct_fields {
                field_map.insert(sf.name.as_str(), &sf.typ);
            }

            let mut provided_fields = std::collections::HashSet::new();
            // Normalize type_args: convert TStruct("T") → TGenericParam("T")
            let normalized_type_args: Vec<Typ> = struct_type_params
                .get(name.as_str())
                .map(|stp| type_args.iter().map(|ta| normalize_typ(ta, stp)).collect())
                .unwrap_or_else(|| type_args.clone());
            // Build type substitution from the struct's type_params to the normalized type_args
            let type_subst: HashMap<String, Typ> = struct_type_params
                .get(name.as_str())
                .map(|stp| {
                    stp.iter()
                        .enumerate()
                        .filter_map(|(i, tp)| {
                            normalized_type_args
                                .get(i)
                                .map(|ta| (tp.clone(), ta.clone()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            for vf in fields {
                provided_fields.insert(vf.name.as_str());
                let (ft, fe) = require_type(
                    env,
                    func_sigs,
                    structs,
                    struct_type_params,
                    enums,
                    enum_type_params,
                    func_return,
                    droppable,
                    &vf.value,
                );
                errs.extend(fe);
                match field_map.get(vf.name.as_str()) {
                    Some(expected_t) => {
                        let resolved = if type_subst.is_empty() {
                            (*expected_t).clone()
                        } else {
                            resolve_type(expected_t, &type_subst)
                        };
                        if !types_equal(&resolved, &ft) {
                            errs.push(Error::new(
                                "E0018",
                                loc,
                                &format!("field '{}' of struct '{}' has wrong type", vf.name, name),
                            ));
                        }
                    }
                    None => {
                        errs.push(Error::new(
                            "E0018",
                            loc,
                            &format!("unknown field '{}' in struct '{}'", vf.name, name),
                        ));
                    }
                }
            }

            for sf in struct_fields {
                if !provided_fields.contains(sf.name.as_str()) {
                    errs.push(Error::new(
                        "E0018",
                        loc,
                        &format!("missing field '{}' in struct '{}' literal", sf.name, name),
                    ));
                }
            }

            // Normalize type_args using the struct's type_params
            let normalized_type_args: Vec<Typ> = struct_type_params
                .get(name.as_str())
                .map(|stp| type_args.iter().map(|ta| normalize_typ(ta, stp)).collect())
                .unwrap_or_else(|| type_args.clone());
            (
                Typ::TStruct {
                    name: name.clone(),
                    fields: struct_fields.clone(),
                    type_args: normalized_type_args,
                },
                errs,
            )
        }
        Expr::EFieldAccess { expr, field, loc } => {
            if let Expr::EVar { name: ev_name, .. } = expr.as_ref() {
                if let Some(variants) = enums.get(ev_name) {
                    if variants.iter().any(|v| v.name == *field) {
                        return (
                            Typ::TStruct {
                                name: ev_name.clone(),
                                fields: vec![],
                                type_args: vec![],
                            },
                            vec![],
                        );
                    }
                }
            }
            let (et, mut errs) = require_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                expr,
            );
            match et {
                Typ::TStruct { name: sname, .. } => match structs.get(&sname) {
                    Some(fields) => {
                        for f in fields {
                            if f.name == *field {
                                return (f.typ.clone(), errs);
                            }
                        }
                        errs.push(Error::new(
                            "E0019",
                            loc,
                            &format!("unknown field '{}' in struct '{}'", field, sname),
                        ));
                        (Typ::TInvalid, errs)
                    }
                    None => {
                        // Not a struct — check if it's an enum (numeric field access on payload)
                        if let Some(variants) = enums.get(&sname) {
                            if let Ok(idx) = field.parse::<usize>() {
                                // Numeric payload field access on an enum discriminant
                                // (`s.0`, `s.1`, ...). At typecheck time we don't know which
                                // variant `s` currently holds, so accept the index if ANY
                                // variant's payload is long enough. Use the field type from the
                                // variant with the most payload fields (the "widest" variant),
                                // which is the best static approximation of the runtime type.
                                let widest = variants
                                    .iter()
                                    .filter(|v| idx < v.payload.len())
                                    .max_by_key(|v| v.payload.len());
                                if let Some(v) = widest {
                                    return (v.payload[idx].clone(), errs);
                                }
                                errs.push(Error::new(
                                    "E0019",
                                    loc,
                                    &format!(
                                        "payload index {} out of bounds for enum '{}'",
                                        idx, sname
                                    ),
                                ));
                                (Typ::TInvalid, errs)
                            } else {
                                errs.push(Error::new(
                                    "E0019",
                                    loc,
                                    &format!("unknown variant '{}' in enum '{}'", field, sname),
                                ));
                                (Typ::TInvalid, errs)
                            }
                        } else {
                            errs.push(Error::new(
                                "E0018",
                                loc,
                                &format!("unknown struct '{}'", sname),
                            ));
                            (Typ::TInvalid, errs)
                        }
                    }
                },
                _ => {
                    if let Typ::TTuple { elems } = &et {
                        if let Ok(idx) = field.parse::<usize>() {
                            if idx < elems.len() {
                                return (elems[idx].clone(), errs);
                            }
                        }
                        errs.push(Error::new(
                            "E0019",
                            loc,
                            &format!("tuple index '{}' out of bounds", field),
                        ));
                        (Typ::TInvalid, errs)
                    } else {
                        errs.push(Error::new("E0014", loc, "field access on non-struct type"));
                        (Typ::TInvalid, errs)
                    }
                }
            }
        }
        Expr::ETupleLit { values, loc } => {
            let mut errs = vec![];
            let elem_types: Vec<Typ> = values
                .iter()
                .map(|v| {
                    infer_type(
                        env,
                        func_sigs,
                        structs,
                        struct_type_params,
                        enums,
                        enum_type_params,
                        func_return,
                        droppable,
                        v,
                    )
                    .0
                })
                .collect();
            (Typ::TTuple { elems: elem_types }, errs)
        }
        Expr::EArrayLit { values, loc } => {
            if values.is_empty() {
                return (
                    Typ::TArray {
                        of: Box::new(Typ::TInvalid),
                    },
                    vec![],
                );
            }
            let mut errs = vec![];
            let (first_t, fe) = require_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                &values[0],
            );
            errs.extend(fe);
            let mut all_same = true;
            for v in &values[1..] {
                let (vt, ve) = require_type(
                    env,
                    func_sigs,
                    structs,
                    struct_type_params,
                    enums,
                    enum_type_params,
                    func_return,
                    droppable,
                    v,
                );
                errs.extend(ve);
                if !types_equal(&first_t, &vt) {
                    all_same = false;
                }
            }
            if !all_same {
                errs.push(Error::new(
                    "E0024",
                    loc,
                    "all array elements must have the same type",
                ));
            }
            (
                Typ::TArray {
                    of: Box::new(first_t),
                },
                errs,
            )
        }
        Expr::ECast { expr, to, loc } => {
            let (et, mut errs) = require_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                expr,
            );
            let valid = match (&et, to) {
                (Typ::TInt, Typ::TFloat32)
                | (Typ::TFloat32, Typ::TInt)
                | (Typ::TInt, Typ::TFloat64)
                | (Typ::TFloat64, Typ::TInt)
                | (Typ::TFloat32, Typ::TFloat64)
                | (Typ::TFloat64, Typ::TFloat32)
                | (Typ::TInt, Typ::TChar)
                | (Typ::TChar, Typ::TInt)
                | (Typ::TBool, Typ::TInt)
                // Integer/pointer reinterpret: lets vec.miva materialise a
                // null `ptrany` via `0 as ptrany` and recover an `int` handle
                // from a `ptrany` when needed. The C++ backend lowers both
                // through `static_cast<void*>(intptr_t)` / casts back.
                | (Typ::TInt, Typ::TPtrAny)
                | (Typ::TPtrAny, Typ::TInt)
                // Opaque ptrany → typed ptr<T>: lets vec.miva turn the raw
                // byte-offset pointer from std.mem.offset into a typed slot
                // for ptr_set/deref. reinterpret_cast<void*> in cxx.rs.
                | (Typ::TPtrAny, Typ::TPtr { .. }) => true,
                _ if types_equal(&et, to) => true,
                _ => false,
            };
            if !valid {
                errs.push(Error::new("E0021", loc, &format!("invalid cast")));
            }
            (to.clone(), errs)
        }
        Expr::EBlock {
            stmts,
            result,
            loc: _,
        } => {
            let mut errs = vec![];
            for stmt in stmts {
                match stmt {
                    Stmt::SLetTuple { patterns, expr, .. } => {
                        let (t, se) = require_type(
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                            expr,
                        );
                        errs.extend(se);
                        for name in patterns {
                            env.vars.insert(name.clone(), t.clone());
                        }
                    }
                    Stmt::SLet { name, expr, .. } => {
                        let (t, se) = require_type(
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                            expr,
                        );
                        errs.extend(se);
                        env.vars.insert(name.clone(), t);
                    }
                    Stmt::SLetTyped {
                        name,
                        typ,
                        expr,
                        loc,
                    } => {
                        let (t, se) = require_type(
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                            expr,
                        );
                        errs.extend(se);
                        if !types_equal(typ, &t) {
                            errs.push(Error::new(
                                "E0022",
                                loc,
                                &format!(
                                    "type mismatch in let: declared {:?} but expression has type {:?}",
                                    typ, t
                                ),
                            ));
                        }
                        env.vars.insert(name.clone(), typ.clone());
                    }
                    Stmt::SAssign { name, expr, loc } => {
                        let (t, se) = require_type(
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                            expr,
                        );
                        errs.extend(se);
                        match env.vars.get(name.as_str()) {
                            Some(expected_t) => {
                                if !types_equal(expected_t, &t) {
                                    errs.push(Error::new(
                                        "E0022",
                                        loc,
                                        &format!("cannot assign to variable '{}'", name),
                                    ));
                                }
                            }
                            None => {}
                        }
                    }
                    Stmt::SFieldAssign {
                        target,
                        field,
                        expr,
                        loc,
                    } => {
                        // `target.field = expr`: require target to be a struct,
                        // field to exist with type F, and expr to have type F.
                        let (tt, te) = require_type(
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                            target,
                        );
                        errs.extend(te);
                        let (et, ee) = require_type(
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                            expr,
                        );
                        errs.extend(ee);
                        if let Typ::TStruct { name, .. } = &tt {
                            match structs.get(name) {
                                Some(fields) => match fields.iter().find(|f| f.name == *field) {
                                    Some(f) => {
                                        if !types_equal(&f.typ, &et) {
                                            errs.push(Error::new(
                                                "E0014",
                                                loc,
                                                &format!(
                                                    "field '{}' of struct '{}' has wrong type",
                                                    field, name
                                                ),
                                            ));
                                        }
                                    }
                                    None => {
                                        errs.push(Error::new(
                                            "E0014",
                                            loc,
                                            &format!("struct '{}' has no field '{}'", name, field),
                                        ));
                                    }
                                },
                                None => {}
                            }
                        } else {
                            errs.push(Error::new(
                                "E0014",
                                loc,
                                "field assignment target is not a struct",
                            ));
                        }
                    }
                    Stmt::SReturn { expr, loc } => {
                        let (t, se) = require_type(
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                            expr,
                        );
                        errs.extend(se);
                        if let Some(ref rt) = func_return {
                            if !types_equal(rt, &t) {
                                errs.push(Error::new(
                                    "E0017",
                                    loc,
                                    &format!("return type incorrect"),
                                ));
                            }
                        }
                    }
                    Stmt::SExpr { expr, .. } => {
                        let (_, se) = infer_type(
                            env,
                            func_sigs,
                            structs,
                            struct_type_params,
                            enums,
                            enum_type_params,
                            func_return,
                            droppable,
                            expr,
                        );
                        errs.extend(se);
                    }
                    Stmt::SCIntro { .. } | Stmt::SEmpty { .. } => {}
                }
            }
            if let Some(r) = result {
                let (t, se) = infer_type(
                    env,
                    func_sigs,
                    structs,
                    struct_type_params,
                    enums,
                    enum_type_params,
                    func_return,
                    droppable,
                    r,
                );
                errs.extend(se);
                (t, errs)
            } else {
                (Typ::TNull, errs)
            }
        }
        Expr::EWhile { cond, body, loc } => {
            let mut errs = vec![];
            let (ct, ce) = require_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                cond,
            );
            errs.extend(ce);
            if !types_equal(&ct, &Typ::TBool) {
                errs.push(Error::new(
                    "E0014",
                    loc,
                    &format!("while condition must be bool"),
                ));
            }
            let (_, be) = infer_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                body,
            );
            errs.extend(be);
            (Typ::TNull, errs)
        }
        Expr::ELoop { body, .. } => {
            let mut errs = vec![];
            let (_, be) = infer_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                body,
            );
            errs.extend(be);
            (Typ::TNull, errs)
        }
        Expr::EFor {
            var,
            range,
            body,
            loc,
        } => {
            let mut errs = vec![];
            let (rt, re) = require_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                range,
            );
            errs.extend(re);
            let elem_type = match rt {
                Typ::TArray { ref of } => of.as_ref().clone(),
                _ => {
                    errs.push(Error::new(
                        "E0026",
                        loc,
                        &format!("for loop range must be an array"),
                    ));
                    Typ::TInvalid
                }
            };
            env.vars.insert(var.clone(), elem_type);
            let (_, be) = infer_type(
                env,
                func_sigs,
                structs,
                struct_type_params,
                enums,
                enum_type_params,
                func_return,
                droppable,
                body,
            );
            errs.extend(be);
            (Typ::TNull, errs)
        }
        Expr::EMacro { .. } => (Typ::TNull, vec![]),
        Expr::EMacroVar { .. } => unreachable!(),
        Expr::EMethodCall { .. } => unreachable!(),
    }
}
