use super::*;

pub fn optimize_expr(expr: IrExpr) -> IrExpr {
    match expr {
        IrExpr::BinOp { op, left, right } => {
            let l = optimize_expr(*left);
            let r = optimize_expr(*right);
            match (&l, &r) {
                (IrExpr::Int(a), IrExpr::Int(b)) => match op {
                    BinOp::Add => IrExpr::Int(a + b),
                    BinOp::Sub => IrExpr::Int(a - b),
                    BinOp::Mul => IrExpr::Int(a * b),
                    BinOp::Div => IrExpr::Int(a / b),
                    _ => IrExpr::BinOp {
                        op,
                        left: Box::new(IrExpr::Int(*a)),
                        right: Box::new(IrExpr::Int(*b)),
                    },
                },
                (IrExpr::Bool(a), IrExpr::Bool(b)) => match op {
                    BinOp::And => IrExpr::Bool(*a && *b),
                    BinOp::Or => IrExpr::Bool(*a || *b),
                    BinOp::Eq => IrExpr::Bool(*a == *b),
                    BinOp::Neq => IrExpr::Bool(*a != *b),
                    _ => IrExpr::BinOp {
                        op,
                        left: Box::new(IrExpr::Bool(*a)),
                        right: Box::new(IrExpr::Bool(*b)),
                    },
                },
                _ => IrExpr::BinOp {
                    op,
                    left: Box::new(l),
                    right: Box::new(r),
                },
            }
        }
        IrExpr::IfValue {
            cond,
            then,
            else_,
            has_panic,
        } => {
            let c = optimize_expr(*cond);
            match c {
                IrExpr::Bool(true) => optimize_expr(*then),
                IrExpr::Bool(false) => {
                    if let Some(e) = else_ {
                        optimize_expr(*e)
                    } else {
                        IrExpr::Void
                    }
                }
                _ => IrExpr::IfValue {
                    cond: Box::new(c),
                    then: optimize_expr_box(then),
                    else_: else_.map(|e| optimize_expr_box(e)),
                    has_panic,
                },
            }
        }
        IrExpr::Block { stmts, result } => {
            let opt_stmts: Vec<IrStmt> = stmts.into_iter().flat_map(optimize_stmt).collect();
            let opt_result = result.map(|e| Box::new(optimize_expr(*e)));
            IrExpr::Block {
                stmts: opt_stmts,
                result: opt_result,
            }
        }
        IrExpr::While { cond, body, result } => IrExpr::While {
            cond: Box::new(optimize_expr(*cond)),
            body: body.into_iter().flat_map(optimize_stmt).collect(),
            result: result.map(|e| Box::new(optimize_expr(*e))),
        },
        IrExpr::Loop { body, result } => IrExpr::Loop {
            body: body.into_iter().flat_map(optimize_stmt).collect(),
            result: result.map(|e| Box::new(optimize_expr(*e))),
        },
        IrExpr::For {
            var,
            range,
            body,
            result,
        } => IrExpr::For {
            var,
            range: Box::new(optimize_expr(*range)),
            body: body.into_iter().flat_map(optimize_stmt).collect(),
            result: result.map(|e| Box::new(optimize_expr(*e))),
        },
        IrExpr::Choose {
            var,
            cases,
            otherwise,
            has_panic,
        } => IrExpr::Choose {
            var: Box::new(optimize_expr(*var)),
            cases: cases
                .into_iter()
                .map(|c| IrCase {
                    pattern: c.pattern,
                    guard: c.guard.map(|g| optimize_expr(g)),
                    then: optimize_expr(c.then),
                })
                .collect(),
            otherwise: otherwise.map(|e| Box::new(optimize_expr(*e))),
            has_panic,
        },
        IrExpr::Cast { expr, to } => {
            let e = optimize_expr(*expr);
            // Fold cast of literal
            match (&e, &to) {
                (IrExpr::Int(v), Typ::TInt) => IrExpr::Int(*v),
                (IrExpr::Float(v), Typ::TFloat64 | Typ::TFloat32) => IrExpr::Float(*v),
                _ => IrExpr::Cast {
                    expr: Box::new(e),
                    to,
                },
            }
        }
        IrExpr::Addr(e) => IrExpr::Addr(Box::new(optimize_expr(*e))),
        IrExpr::Deref(e) => IrExpr::Deref(Box::new(optimize_expr(*e))),
        IrExpr::FieldAccess { expr, field } => IrExpr::FieldAccess {
            expr: Box::new(optimize_expr(*expr)),
            field,
        },
        IrExpr::Call {
            name,
            type_args,
            args,
        } => IrExpr::Call {
            name,
            type_args,
            args: args.into_iter().map(optimize_expr).collect(),
        },
        IrExpr::ArrayInit(values) => {
            IrExpr::ArrayInit(values.into_iter().map(optimize_expr).collect())
        }
        IrExpr::TupleInit(values) => {
            IrExpr::TupleInit(values.into_iter().map(optimize_expr).collect())
        }
        IrExpr::StructInit {
            name,
            type_args,
            fields,
        } => IrExpr::StructInit {
            name,
            type_args,
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, optimize_expr(e)))
                .collect(),
        },
        IrExpr::Macro { name, args } => IrExpr::Macro {
            name,
            args: args.into_iter().map(optimize_expr).collect(),
        },
        other => other,
    }
}

pub(crate) fn optimize_expr_box(e: Box<IrExpr>) -> Box<IrExpr> {
    Box::new(optimize_expr(*e))
}

pub(crate) fn flatten_block(
    stmts: Vec<IrStmt>,
    result: Option<Box<IrExpr>>,
) -> (Vec<IrStmt>, Option<Box<IrExpr>>) {
    let mut flat = Vec::new();
    for s in stmts {
        match s {
            IrStmt::Expr(IrExpr::Block {
                stmts: inner_stmts,
                result: inner_result,
            }) => {
                let (inner_flat, inner_res) = flatten_block(inner_stmts, inner_result);
                flat.extend(inner_flat);
                if let Some(r) = inner_res {
                    flat.push(IrStmt::Return(*r));
                }
            }
            IrStmt::Return(IrExpr::Block {
                stmts: inner_stmts,
                result: inner_result,
            }) => {
                let (inner_flat, inner_res) = flatten_block(inner_stmts, inner_result);
                flat.extend(inner_flat);
                if let Some(r) = inner_res {
                    flat.push(IrStmt::Return(*r));
                }
            }
            other => flat.extend(optimize_stmt(other)),
        }
    }
    (flat, result)
}

pub fn optimize_stmt(stmt: IrStmt) -> Vec<IrStmt> {
    match stmt {
        IrStmt::Let {
            mutable,
            name,
            expr,
        } => {
            vec![IrStmt::Let {
                mutable,
                name,
                expr: optimize_expr(expr),
            }]
        }
        IrStmt::LetTyped { name, typ, expr } => {
            vec![IrStmt::LetTyped {
                name,
                typ,
                expr: optimize_expr(expr),
            }]
        }
        IrStmt::Return(e) => vec![IrStmt::Return(optimize_expr(e))],
        IrStmt::Expr(e) => {
            let opt = optimize_expr(e);
            if is_noop_expr(&opt) {
                vec![]
            } else {
                vec![IrStmt::Expr(opt)]
            }
        }
        IrStmt::Assign { name, expr } => {
            vec![IrStmt::Assign {
                name,
                expr: optimize_expr(expr),
            }]
        }
        IrStmt::FieldAssign {
            target,
            field,
            expr,
        } => {
            vec![IrStmt::FieldAssign {
                target: optimize_expr(target),
                field,
                expr: optimize_expr(expr),
            }]
        }
        IrStmt::If { cond, then, else_ } => {
            let c = optimize_expr(cond);
            match c {
                IrExpr::Bool(true) => then
                    .into_iter()
                    .flat_map(optimize_stmt)
                    .filter(|s| !matches!(s, IrStmt::Empty))
                    .collect(),
                IrExpr::Bool(false) => else_.into_iter().flat_map(optimize_stmt).collect(),
                _ => vec![IrStmt::If {
                    cond: c,
                    then: then.into_iter().flat_map(optimize_stmt).collect(),
                    else_: else_.into_iter().flat_map(optimize_stmt).collect(),
                }],
            }
        }
        IrStmt::While { cond, body } => {
            vec![IrStmt::While {
                cond: optimize_expr(cond),
                body: body.into_iter().flat_map(optimize_stmt).collect(),
            }]
        }
        IrStmt::Loop { body } => {
            vec![IrStmt::Loop {
                body: body.into_iter().flat_map(optimize_stmt).collect(),
            }]
        }
        IrStmt::For { var, range, body } => {
            vec![IrStmt::For {
                var,
                range: optimize_expr(range),
                body: body.into_iter().flat_map(optimize_stmt).collect(),
            }]
        }
        IrStmt::Empty => vec![],
    }
}

pub(crate) fn is_noop_expr(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Void)
}

pub fn optimize_def(def: IrDef) -> IrDef {
    match def {
        IrDef::Func {
            name,
            type_params,
            params,
            returns,
            body_stmts,
            body_result,
            is_async,
        } => {
            let opt_stmts: Vec<IrStmt> = body_stmts.into_iter().flat_map(optimize_stmt).collect();
            let opt_result = body_result.map(optimize_expr);
            IrDef::Func {
                name,
                type_params,
                params,
                returns,
                body_stmts: opt_stmts,
                body_result: opt_result,
                is_async,
            }
        }
        IrDef::AsyncFunc {
            name,
            type_params,
            params,
            returns,
            body_stmts,
            body_result,
        } => {
            let opt_stmts: Vec<IrStmt> = body_stmts.into_iter().flat_map(optimize_stmt).collect();
            let opt_result = body_result.map(optimize_expr);
            IrDef::AsyncFunc {
                name,
                type_params,
                params,
                returns,
                body_stmts: opt_stmts,
                body_result: opt_result,
            }
        }
        IrDef::Test {
            name,
            body_stmts,
            body_result,
        } => {
            let opt_stmts: Vec<IrStmt> = body_stmts.into_iter().flat_map(optimize_stmt).collect();
            let opt_result = body_result.map(optimize_expr);
            IrDef::Test {
                name,
                body_stmts: opt_stmts,
                body_result: opt_result,
            }
        }
        IrDef::Module { name, defs } => IrDef::Module {
            name,
            defs: defs.into_iter().map(optimize_def).collect(),
        },
        other => other,
    }
}

pub fn optimize_defs(defs: Vec<IrDef>) -> Vec<IrDef> {
    defs.into_iter().map(optimize_def).collect()
}

// ===== EMITTER: IR → C++ source =====
