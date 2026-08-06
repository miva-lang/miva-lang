use crate::ast::*;
use crate::error::Error;

pub(crate) fn seal_check_expr(
    expr: &Expr,
    sealed: &std::collections::HashSet<&str>,
    errs: &mut Vec<Error>,
) {
    match expr {
        Expr::EVar { loc, name } => {
            if sealed.contains(name.as_str()) {
                errs.push(Error::new(
                    "E0034",
                    loc,
                    &format!(
                        "drop function '{}' is sealed and cannot be used as a value",
                        name
                    ),
                ));
            }
        }
        Expr::ECall {
            loc, name, args, ..
        } => {
            if sealed.contains(name.as_str()) {
                errs.push(Error::new(
                    "E0034",
                    loc,
                    &format!("drop function '{}' is sealed and cannot be called directly; use drop(x) instead", name),
                ));
            }
            for a in args {
                seal_check_expr(a, sealed, errs);
            }
        }
        Expr::EMethodCall { expr, args, .. } => {
            seal_check_expr(expr, sealed, errs);
            for a in args {
                seal_check_expr(a, sealed, errs);
            }
        }
        Expr::EMacro { args, .. } => {
            for a in args {
                seal_check_expr(a, sealed, errs);
            }
        }
        Expr::EStructLit { fields, .. } => {
            for f in fields {
                seal_check_expr(&f.value, sealed, errs);
            }
        }
        Expr::EFieldAccess { expr, .. }
        | Expr::ECast { expr, .. }
        | Expr::EAddr { expr, .. }
        | Expr::EDeref { expr, .. } => seal_check_expr(expr, sealed, errs),
        Expr::EBinOp { left, right, .. } => {
            seal_check_expr(left, sealed, errs);
            seal_check_expr(right, sealed, errs);
        }
        Expr::EIf {
            cond, then, else_, ..
        } => {
            seal_check_expr(cond, sealed, errs);
            seal_check_expr(then, sealed, errs);
            if let Some(e) = else_ {
                seal_check_expr(e, sealed, errs);
            }
        }
        Expr::EChoose {
            var,
            cases,
            otherwise,
            ..
        } => {
            seal_check_expr(var, sealed, errs);
            for c in cases {
                seal_check_expr(&c.when, sealed, errs);
                if let Some(g) = &c.guard {
                    seal_check_expr(g, sealed, errs);
                }
                seal_check_expr(&c.then, sealed, errs);
            }
            if let Some(o) = otherwise {
                seal_check_expr(o, sealed, errs);
            }
        }
        Expr::EBlock { stmts, result, .. } => {
            for s in stmts {
                seal_check_stmt(s, sealed, errs);
            }
            if let Some(r) = result {
                seal_check_expr(r, sealed, errs);
            }
        }
        Expr::EArrayLit { values, .. } => {
            for v in values {
                seal_check_expr(v, sealed, errs);
            }
        }
        Expr::ETupleLit { values, .. } => {
            for v in values {
                seal_check_expr(v, sealed, errs);
            }
        }
        Expr::EWhile { cond, body, .. } => {
            seal_check_expr(cond, sealed, errs);
            seal_check_expr(body, sealed, errs);
        }
        Expr::ELoop { body, .. } => seal_check_expr(body, sealed, errs),
        Expr::EFor { range, body, .. } => {
            seal_check_expr(range, sealed, errs);
            seal_check_expr(body, sealed, errs);
        }
        Expr::ELambda { body, .. } => seal_check_expr(body, sealed, errs),
        Expr::EInt { .. }
        | Expr::EBool { .. }
        | Expr::EFloat { .. }
        | Expr::EChar { .. }
        | Expr::EString { .. }
        | Expr::EMove { .. }
        | Expr::EClone { .. }
        | Expr::EMacroVar { .. }
        | Expr::EEnumPattern { .. }
        | Expr::EVoid { .. }
        | Expr::ETupleLit { .. } => {}
    }
}

pub(crate) fn seal_check_stmt(
    stmt: &Stmt,
    sealed: &std::collections::HashSet<&str>,
    errs: &mut Vec<Error>,
) {
    match stmt {
        Stmt::SLet { expr, .. }
        | Stmt::SLetTuple { expr, .. }
        | Stmt::SLetTyped { expr, .. }
        | Stmt::SAssign { expr, .. }
        | Stmt::SReturn { expr, .. }
        | Stmt::SExpr { expr, .. } => seal_check_expr(expr, sealed, errs),
        Stmt::SFieldAssign { target, expr, .. } => {
            seal_check_expr(target, sealed, errs);
            seal_check_expr(expr, sealed, errs);
        }
        Stmt::SCIntro { .. } | Stmt::SEmpty { .. } => {}
    }
}
