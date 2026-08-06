use super::*;

pub(crate) fn lower_expr(ctx: &mut IrContext, expr: &Expr) -> IrExpr {
    match expr {
        Expr::EInt { value, .. } => IrExpr::Int(*value),
        Expr::EBool { value, .. } => IrExpr::Bool(*value),
        Expr::EFloat { value, .. } => IrExpr::Float(*value),
        Expr::EChar { value, .. } => IrExpr::Char(value.clone()),
        Expr::EString { value, .. } => IrExpr::String(value.clone()),
        Expr::EVoid { .. } => IrExpr::Void,
        Expr::EVar { name, .. } => IrExpr::Var(name.clone()),
        Expr::EMove { name, .. } => IrExpr::Move(name.clone()),
        Expr::EClone { name, .. } => IrExpr::Clone(name.clone()),
        Expr::EStructLit { name, fields, type_args, .. } => {
            let lowered_fields: Vec<_> = fields.iter().map(|f| {
                (f.name.clone(), lower_expr(ctx, &f.value))
            }).collect();
            IrExpr::StructInit { name: name.clone(), type_args: type_args.clone(), fields: lowered_fields }
        }
        Expr::EFieldAccess { expr, field, .. } => {
            let lowered_expr = lower_expr(ctx, expr);
            IrExpr::FieldAccess { expr: Box::new(lowered_expr), field: field.clone() }
        }
        Expr::EBinOp { op, left, right, .. } => {
            IrExpr::BinOp { op: op.clone(), left: Box::new(lower_expr(ctx, left)), right: Box::new(lower_expr(ctx, right)) }
        }
        Expr::EIf { cond, then, else_, .. } => {
            let has_panic = else_.as_ref().map_or(false, |e| is_panic(e))
                || is_panic(then);
            IrExpr::IfValue {
                cond: Box::new(lower_expr(ctx, cond)),
                then: Box::new(lower_expr(ctx, then)),
                else_: else_.as_ref().map(|e| Box::new(lower_expr(ctx, e))),
                has_panic,
            }
        }
        Expr::EWhile { cond, body, .. } => {
            let (body_stmts, body_result) = lower_block(ctx, body);
            IrExpr::While {
                cond: Box::new(lower_expr(ctx, cond)),
                body: body_stmts,
                result: body_result.map(Box::new),
            }
        }
        Expr::ELoop { body, .. } => {
            let (body_stmts, body_result) = lower_block(ctx, body);
            IrExpr::Loop {
                body: body_stmts,
                result: body_result.map(Box::new),
            }
        }
        Expr::EFor { var, range, body, .. } => {
            let (body_stmts, body_result) = lower_block(ctx, body);
            IrExpr::For {
                var: var.clone(),
                range: Box::new(lower_expr(ctx, range)),
                body: body_stmts,
                result: body_result.map(Box::new),
            }
        }
        Expr::ECall { name, type_args, args, .. } => {
            let lowered_args: Vec<_> = args.iter().map(|a| lower_expr(ctx, a)).collect();
            IrExpr::Call { name: name.clone(), type_args: type_args.clone(), args: lowered_args }
        }
        Expr::ECast { expr, to, .. } => {
            IrExpr::Cast { expr: Box::new(lower_expr(ctx, expr)), to: to.clone() }
        }
        Expr::EBlock { stmts, result, .. } => {
            if result.is_some() {
                let lowered_stmts: Vec<IrStmt> = stmts.iter().flat_map(|s| lower_stmt(ctx, s)).collect();
                IrExpr::Block { stmts: lowered_stmts, result: result.as_ref().map(|e| Box::new(lower_expr(ctx, e))) }
            } else if stmts.last().map_or(false, |s| matches!(s, Stmt::SExpr { .. })) {
                let (non_last, last) = stmts.split_at(stmts.len() - 1);
                let lowered_stmts: Vec<IrStmt> = non_last.iter().flat_map(|s| lower_stmt(ctx, s)).collect();
                if let Stmt::SExpr { expr, .. } = &last[0] {
                    IrExpr::Block { stmts: lowered_stmts, result: Some(Box::new(lower_expr(ctx, expr))) }
                } else {
                    unreachable!()
                }
            } else {
                let lowered_stmts: Vec<IrStmt> = stmts.iter().flat_map(|s| lower_stmt(ctx, s)).collect();
                IrExpr::Block { stmts: lowered_stmts, result: None }
            }
        }
        Expr::EArrayLit { values, .. } => {
            let lowered: Vec<_> = values.iter().map(|v| lower_expr(ctx, v)).collect();
            IrExpr::ArrayInit(lowered)
        }
        Expr::EAddr { expr, .. } => {
            IrExpr::Addr(Box::new(lower_expr(ctx, expr)))
        }
        Expr::EDeref { expr, .. } => {
            IrExpr::Deref(Box::new(lower_expr(ctx, expr)))
        }
        Expr::EMacro { name, args, .. } => {
            let lowered_args: Vec<_> = args.iter().map(|a| lower_expr(ctx, a)).collect();
            IrExpr::Macro { name: name.clone(), args: lowered_args }
        }
        Expr::EMacroVar { .. } => unreachable!(),
        Expr::EMethodCall { .. } => unreachable!(),
        Expr::EEnumPattern { .. } => {
            unreachable!("EEnumPattern is handled inline in the EChoose arm")
        }
        Expr::ETupleLit { values, .. } => {
            let lowered: Vec<_> = values.iter().map(|v| lower_expr(ctx, v)).collect();
            IrExpr::TupleInit(lowered)
        }
        Expr::ELambda { params, ret, captures, body, .. } => {
            let id = ctx.next_id();
            let ret_cxx = cxx_type(ret);
            let param_strs: Vec<_> = params.iter().map(cxx_param).collect();
            let param_list = param_strs.join(", ");

            let env_fields: Vec<_> = captures
                .iter()
                .map(|(n, t)| (n.clone(), t.clone()))
                .collect();

            let (body_stmts, body_result) = lower_block(ctx, body);

            let inner_tys: Vec<_> = params.iter().map(|p| match p {
                Param::PRef { typ, .. } | Param::POwn { typ, .. } => cxx_type(typ),
            }).collect();
            let closure_ty = if inner_tys.is_empty() {
                format!("mvp_closure<{}>", ret_cxx)
            } else {
                format!("mvp_closure<{}, {}>", ret_cxx, inner_tys.join(", "))
            };

            emit_closure_def(IrClosureDef {
                id,
                env_fields,
                thunk_sig: format!("static {} __closure_thunk_{}(void* __env_ptr, {})", ret_cxx, id, param_list),
                param_list,
                param_types: inner_tys,
                thunk_body_stmts: body_stmts,
                thunk_body_result: body_result,
                ret_type: ret.clone(),
            });

            IrExpr::ClosureRef { id }
        }
        Expr::EChoose { var, cases, otherwise, .. } => {
            let has_panic = cases.iter().any(|c| is_panic(&c.then))
                || otherwise.as_ref().map_or(false, |e| is_panic(e));
            let lowered_var = lower_expr(ctx, var);
            let lowered_cases: Vec<IrCase> = cases.iter().map(|c| {
                let pattern = match c.when.as_ref() {
                    Expr::EEnumPattern { enum_name, variant, bindings, .. } => {
                        IrPattern::EnumTag {
                            enum_name: enum_name.clone(),
                            variant: variant.clone(),
                            bindings: bindings.clone(),
                        }
                    }
                    Expr::EFieldAccess { expr: inner_expr, field: variant, .. } => {
                        if let Expr::EVar { name: enum_name, .. } = inner_expr.as_ref() {
                            if enum_name.chars().next().map_or(false, |c| c.is_uppercase()) {
                                IrPattern::EnumTag {
                                    enum_name: enum_name.clone(),
                                    variant: variant.clone(),
                                    bindings: Vec::new(),
                                }
                            } else {
                                IrPattern::Value(lower_expr(ctx, c.when.as_ref()))
                            }
                        } else {
                            IrPattern::Value(lower_expr(ctx, c.when.as_ref()))
                        }
                    }
                    _ => IrPattern::Value(lower_expr(ctx, c.when.as_ref())),
                };
                let guard = c.guard.as_ref().map(|g| lower_expr(ctx, g));
                IrCase {
                    pattern,
                    guard,
                    then: lower_expr(ctx, c.then.as_ref()),
                }
            }).collect();
            let lowered_otherwise = otherwise.as_ref().map(|e| Box::new(lower_expr(ctx, e)));
            IrExpr::Choose {
                var: Box::new(lowered_var),
                cases: lowered_cases,
                otherwise: lowered_otherwise,
                has_panic,
            }
        }
    }
}

pub(crate) fn lower_block(ctx: &mut IrContext, expr: &Expr) -> (Vec<IrStmt>, Option<IrExpr>) {
    match expr {
        Expr::EBlock { stmts, result, .. } => {
            if result.is_some() {
                let lowered_stmts: Vec<IrStmt> = stmts.iter().flat_map(|s| lower_stmt(ctx, s)).collect();
                (lowered_stmts, result.as_ref().map(|e| lower_expr(ctx, e)))
            } else if stmts.last().map_or(false, |s| matches!(s, Stmt::SExpr { .. })) {
                let (non_last, last) = stmts.split_at(stmts.len() - 1);
                let lowered_stmts: Vec<IrStmt> = non_last.iter().flat_map(|s| lower_stmt(ctx, s)).collect();
                if let Stmt::SExpr { expr, .. } = &last[0] {
                    (lowered_stmts, Some(lower_expr(ctx, expr)))
                } else {
                    unreachable!()
                }
            } else {
                let lowered_stmts: Vec<IrStmt> = stmts.iter().flat_map(|s| lower_stmt(ctx, s)).collect();
                (lowered_stmts, None)
            }
        }
        _ => (Vec::new(), Some(lower_expr(ctx, expr))),
    }
}

pub(crate) fn lower_stmt(ctx: &mut IrContext, stmt: &Stmt) -> Vec<IrStmt> {
    match stmt {
        Stmt::SLetTuple { patterns, expr, .. } => {
            let tuple_expr = lower_expr(ctx, expr);
            let mut stmts = vec![IrStmt::Let {
                mutable: false,
                name: "__tuple".into(),
                expr: tuple_expr,
            }];
            for (i, name) in patterns.iter().enumerate() {
                stmts.push(IrStmt::Let {
                    mutable: false,
                    name: name.clone(),
                    expr: IrExpr::FieldAccess {
                        expr: Box::new(IrExpr::Var("__tuple".into())),
                        field: i.to_string(),
                    },
                });
            }
            stmts
        }
        Stmt::SLet { mutable, name, expr, .. } => {
            vec![IrStmt::Let { mutable: *mutable, name: name.clone(), expr: lower_expr(ctx, expr) }]
        }
        Stmt::SLetTyped { name, typ, expr, .. } => {
            vec![IrStmt::LetTyped { name: name.clone(), typ: typ.clone(), expr: lower_expr(ctx, expr) }]
        }
        Stmt::SReturn { expr, .. } => {
            vec![IrStmt::Return(lower_expr(ctx, expr))]
        }
        Stmt::SExpr { expr, .. } => {
            // In statement position a loop yields no value, so a trailing
            // expression is still executed for its effects.
            fn loop_body_stmts(ctx: &mut IrContext, body: &Expr) -> Vec<IrStmt> {
                let (mut body_stmts, body_result) = lower_block(ctx, body);
                if let Some(r) = body_result {
                    body_stmts.push(IrStmt::Expr(r));
                }
                body_stmts
            }
            match expr.as_ref() {
                Expr::EWhile { cond, body, .. } => {
                    vec![IrStmt::While {
                        cond: lower_expr(ctx, cond),
                        body: loop_body_stmts(ctx, body),
                    }]
                }
                Expr::ELoop { body, .. } => {
                    vec![IrStmt::Loop { body: loop_body_stmts(ctx, body) }]
                }
                Expr::EFor { var, range, body, .. } => {
                    vec![IrStmt::For {
                        var: var.clone(),
                        range: lower_expr(ctx, range),
                        body: loop_body_stmts(ctx, body),
                    }]
                }
                Expr::EBlock { stmts, result, .. } => {
                    let lowered_stmts: Vec<IrStmt> = stmts.iter().flat_map(|s| lower_stmt(ctx, s)).collect();
                    let lowered_result = result.as_ref().map(|e| Box::new(lower_expr(ctx, e)));
                    vec![IrStmt::Expr(IrExpr::Block { stmts: lowered_stmts, result: lowered_result })]
                }
                _ => vec![IrStmt::Expr(lower_expr(ctx, expr))],
            }
        }
        Stmt::SAssign { name, expr, .. } => {
            vec![IrStmt::Assign { name: name.clone(), expr: lower_expr(ctx, expr) }]
        }
        Stmt::SFieldAssign { target, field, expr, .. } => {
            vec![IrStmt::FieldAssign {
                target: lower_expr(ctx, target),
                field: field.clone(),
                expr: lower_expr(ctx, expr),
            }]
        }
        Stmt::SCIntro { .. } => vec![IrStmt::Empty],
        Stmt::SEmpty { .. } => vec![IrStmt::Empty],
    }
}

pub(crate) fn lower_def(ctx: &mut IrContext, def: &Def) -> IrDef {
    match def {
        Def::DStruct { name, fields, type_params, .. } => {
            IrDef::Struct { name: name.clone(), type_params: type_params.clone(), fields: fields.clone() }
        }
        Def::DEnum { name, variants, type_params, .. } => {
            IrDef::Enum { name: name.clone(), type_params: type_params.clone(), variants: variants.clone() }
        }
        // Shape definitions are treated as regular struct definitions for runtime representation.
        Def::DShape {
            name,
            fields,
            type_params,
            ..
        } => IrDef::Struct {
            name: name.clone(),
            type_params: type_params.clone(),
            fields: fields.clone(),
        },
        Def::DFunc { name, type_params, params, returns, body, is_async, .. } if *is_async => {
            let (body_stmts, body_result) = lower_block(ctx, body);
            IrDef::AsyncFunc {
                name: name.clone(),
                type_params: type_params.clone(),
                params: params.clone(),
                returns: returns.clone(),
                body_stmts,
                body_result,
            }
        }
        Def::DFunc { name, type_params, params, returns, body, .. } => {
            let (body_stmts, body_result) = lower_block(ctx, body);
            IrDef::Func {
                name: name.clone(),
                type_params: type_params.clone(),
                params: params.clone(),
                returns: returns.clone(),
                body_stmts,
                body_result,
                is_async: false,
            }
        }
        Def::DCFuncUnsafe { name, params, returns, code, .. } => {
            IrDef::CFunc { name: name.clone(), params: params.clone(), returns: returns.clone(), code: code.clone() }
        }
        Def::DTest { name, body, .. } => {
            let (body_stmts, body_result) = lower_block(ctx, body);
            IrDef::Test { name: name.clone(), body_stmts, body_result }
        }
        Def::DImpl { struct_name, impls, .. } => {
            IrDef::Impl { struct_name: struct_name.clone(), impls: impls.clone() }
        }
        Def::DMacro { .. } => unreachable!(),
        Def::DModule { .. } => unreachable!("DModule should be handled by lower_defs"),
        Def::SExport { symbol, .. } => IrDef::Export(symbol.clone()),
        Def::SImport { path, .. } => IrDef::Import { path: path.clone() },
        Def::SImportAs { path, alias, .. } => IrDef::ImportAs { path: path.clone(), alias: alias.clone() },
        Def::SImportHere { path, .. } => IrDef::ImportHere { path: path.clone() },
        Def::DCMagical { content, .. } => IrDef::CMagical { content: content.clone() },
        Def::DCIntro { content, .. } => IrDef::CIntro { content: content.clone() },
    }
}

pub(crate) fn lower_defs(ctx: &mut IrContext, defs: &[Def]) -> Vec<IrDef> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < defs.len() {
        match &defs[i] {
            Def::DModule { name, .. } => {
                let mut inner_ctx = IrContext::new();
                let inner_defs = lower_defs(&mut inner_ctx, &defs[i + 1..]);
                out.push(IrDef::Module { name: name.clone(), defs: inner_defs });
                break;
            }
            _ => {
                out.push(lower_def(ctx, &defs[i]));
                i += 1;
            }
        }
    }
    out
}

// ===== OPTIMIZATION PASSES =====

