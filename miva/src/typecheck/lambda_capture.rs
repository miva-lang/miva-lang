use crate::ast::*;
use std::collections::HashMap;
use super::*;

pub(crate) fn free_vars_inside(e: &Expr, bound: &mut Vec<String>, out: &mut Vec<String>) {
    match e {
        Expr::EVar { name, .. } | Expr::EMove { name, .. } | Expr::EClone { name, .. } => {
            if !bound.contains(name) && !out.contains(name) {
                out.push(name.clone());
            }
        }
        Expr::EFieldAccess { expr, .. } => free_vars_inside(expr, bound, out),
        Expr::EBinOp { left, right, .. } => {
            free_vars_inside(left, bound, out);
            free_vars_inside(right, bound, out);
        }
        Expr::EIf { cond, then, else_, .. } => {
            free_vars_inside(cond, bound, out);
            free_vars_inside(then, bound, out);
            if let Some(el) = else_ {
                free_vars_inside(el, bound, out);
            }
        }
        Expr::EChoose { var, cases, otherwise, .. } => {
            free_vars_inside(var, bound, out);
            for c in cases {
                free_vars_inside(&c.then, bound, out);
            }
            if let Some(o) = otherwise {
                free_vars_inside(o, bound, out);
            }
        }
        Expr::ECall { args, .. } => {
            for a in args {
                free_vars_inside(a, bound, out);
            }
        }
        Expr::EMethodCall { expr, args, .. } => {
            free_vars_inside(expr, bound, out);
            for a in args {
                free_vars_inside(a, bound, out);
            }
        }
        Expr::EMacro { args, .. } => {
            for a in args {
                free_vars_inside(a, bound, out);
            }
        }
        Expr::ECast { expr, .. } => free_vars_inside(expr, bound, out),
        Expr::EBlock { stmts, result, .. } => {
            for s in stmts {
                match s {
                    Stmt::SLet { name, expr, .. } | Stmt::SLetTyped { name, expr, .. } => {
                        free_vars_inside(expr, bound, out);
                        bound.push(name.clone());
                    }
                    Stmt::SLetTuple { patterns, expr, .. } => {
                        free_vars_inside(expr, bound, out);
                        for n in patterns {
                            bound.push(n.clone());
                        }
                    }
                    Stmt::SAssign { name, expr, .. } => {
                        if !bound.contains(name) && !out.contains(name) {
                            out.push(name.clone());
                        }
                        free_vars_inside(expr, bound, out);
                    }
                    Stmt::SFieldAssign { target, expr, .. } => {
                        free_vars_inside(target, bound, out);
                        free_vars_inside(expr, bound, out);
                    }
                    Stmt::SReturn { expr, .. } | Stmt::SExpr { expr, .. } => {
                        free_vars_inside(expr, bound, out);
                    }
                    _ => {}
                }
            }
            if let Some(r) = result {
                free_vars_inside(r, bound, out);
            }
        }
        Expr::EArrayLit { values, .. } => {
            for e2 in values {
                free_vars_inside(e2, bound, out);
            }
        }
        Expr::ETupleLit { values, .. } => {
            for e2 in values {
                free_vars_inside(e2, bound, out);
            }
        }
        Expr::EStructLit { fields, .. } => {
            for v in fields {
                free_vars_inside(&v.value, bound, out);
            }
        }
        Expr::EAddr { expr, .. } | Expr::EDeref { expr, .. } => free_vars_inside(expr, bound, out),
        Expr::EWhile { cond, body, .. } => {
            free_vars_inside(cond, bound, out);
            free_vars_inside(body, bound, out);
        }
        Expr::ELoop { body, .. } => free_vars_inside(body, bound, out),
        Expr::EFor { var, range, body, .. } => {
            free_vars_inside(range, bound, out);
            let mut b2 = bound.clone();
            b2.push(var.clone());
            free_vars_inside(body, &mut b2, out);
        }
        Expr::EEnumPattern { .. } => {}
        Expr::ELambda { params, body, .. } => {
            let mut b2 = bound.clone();
            for p in params {
                let n = match p {
                    Param::PRef { name, .. } | Param::POwn { name, .. } => name.clone(),
                };
                b2.push(n);
            }
            free_vars_inside(body, &mut b2, out);
        }
        _ => {}
    }
}

pub(crate) fn compute_lambda_captures(
    env: &TypeEnv,
    params: &[Param],
    body: &Expr,
) -> Vec<(String, Typ)> {
    let mut bound: Vec<String> = params
        .iter()
        .map(|p| match p {
            Param::PRef { name, .. } | Param::POwn { name, .. } => name.clone(),
        })
        .collect();
    let mut out: Vec<String> = vec![];
    free_vars_inside(body, &mut bound, &mut out);
    out.into_iter()
        .filter_map(|n| env.vars.get(&n).map(|t| (n, t.clone())))
        .collect()
}


/// Walk every expression in `defs` and fill each `ELambda.captures` with the
/// free variables it references from the enclosing scope. Mutates the AST in
/// place so backends can read captures directly. Runs after type checking,
/// before codegen.
pub fn annotate_lambda_captures(defs: &mut [Def]) {
    for def in defs.iter_mut() {
        match def {
            Def::DFunc { params, body, .. } => {
                let mut env: HashMap<String, Typ> = HashMap::new();
                for p in params {
                    match p {
                        Param::PRef { name, typ } | Param::POwn { name, typ } => {
                            env.insert(name.clone(), typ.clone());
                        }
                    }
                }
                *body = Box::new(annotate_expr(*body.clone(), &env));
            }
            Def::DTest { body, .. } => {
                let env: HashMap<String, Typ> = HashMap::new();
                *body = Box::new(annotate_expr(*body.clone(), &env));
            }
            _ => {}
        }
    }
}

pub(crate) fn expr_simple_typ(e: &Expr) -> Typ {
    match e {
        Expr::EInt { .. } => Typ::TInt,
        Expr::EBool { .. } => Typ::TBool,
        Expr::EFloat { .. } => Typ::TFloat64,
        Expr::EChar { .. } => Typ::TChar,
        Expr::EString { .. } => Typ::TString,
        Expr::EVoid { .. } => Typ::TNull,
        Expr::ELambda { params, ret, .. } => {
            let param_typs: Vec<Typ> = params
                .iter()
                .map(|p| match p {
                    Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ.clone(),
                })
                .collect();
            Typ::TFunc {
                params: param_typs,
                returns: Box::new(ret.clone()),
            }
        }
        _ => Typ::TInt,
    }
}

pub(crate) fn annotate_expr(e: Expr, env: &HashMap<String, Typ>) -> Expr {
    match e {
        Expr::ELambda {
            loc,
            params,
            ret,
            captures: _,
            body,
        } => {
            let captures = compute_lambda_captures(
                &TypeEnv {
                    vars: env.clone(),
                },
                &params,
                &body,
            );
            let mut child_env = env.clone();
            for p in &params {
                match p {
                    Param::PRef { name, typ } | Param::POwn { name, typ } => {
                        child_env.insert(name.clone(), typ.clone());
                    }
                }
            }
            let new_body = annotate_expr(*body, &child_env);
            Expr::ELambda {
                loc,
                params,
                ret,
                captures,
                body: Box::new(new_body),
            }
        }
        Expr::EFieldAccess { loc, expr, field } => {
            let new_expr = annotate_expr(*expr, env);
            Expr::EFieldAccess {
                loc,
                expr: Box::new(new_expr),
                field,
            }
        }
        Expr::EBinOp { loc, op, left, right } => {
            let new_left = annotate_expr(*left, env);
            let new_right = annotate_expr(*right, env);
            Expr::EBinOp {
                loc,
                op,
                left: Box::new(new_left),
                right: Box::new(new_right),
            }
        }
        Expr::EIf { loc, cond, then, else_ } => {
            let new_cond = annotate_expr(*cond, env);
            let new_then = annotate_expr(*then, env);
            let new_else = else_.map(|e| Box::new(annotate_expr(*e, env)));
            Expr::EIf {
                loc,
                cond: Box::new(new_cond),
                then: Box::new(new_then),
                else_: new_else,
            }
        }
        Expr::EChoose {
            loc,
            var,
            cases,
            otherwise,
        } => {
            let new_var = annotate_expr(*var, env);
            let new_cases = cases
                .into_iter()
                .map(|mut c| {
                    c.then = Box::new(annotate_expr(*c.then, env));
                    c
                })
                .collect();
            let new_otherwise = otherwise.map(|o| Box::new(annotate_expr(*o, env)));
            Expr::EChoose {
                loc,
                var: Box::new(new_var),
                cases: new_cases,
                otherwise: new_otherwise,
            }
        }
        Expr::ECall {
            loc,
            name,
            args,
            type_args,
        } => {
            let new_args = args.into_iter().map(|a| annotate_expr(a, env)).collect();
            Expr::ECall {
                loc,
                name,
                args: new_args,
                type_args,
            }
        }
        Expr::EMethodCall {
            loc,
            expr,
            method,
            type_args,
            args,
        } => {
            let new_expr = annotate_expr(*expr, env);
            let new_args = args.into_iter().map(|a| annotate_expr(a, env)).collect();
            Expr::EMethodCall {
                loc,
                expr: Box::new(new_expr),
                method,
                type_args,
                args: new_args,
            }
        }
        Expr::EMacro { loc, name, args } => {
            let new_args = args.into_iter().map(|a| annotate_expr(a, env)).collect();
            Expr::EMacro {
                loc,
                name,
                args: new_args,
            }
        }
        Expr::ECast { loc, expr, to } => {
            let new_expr = annotate_expr(*expr, env);
            Expr::ECast {
                loc,
                expr: Box::new(new_expr),
                to,
            }
        }
        Expr::EBlock { loc, stmts, result } => {
            let mut child_env = env.clone();
            let mut new_stmts = vec![];
            for s in stmts {
                match s {
                    Stmt::SLetTuple {
                        loc,
                        patterns,
                        expr,
                    } => {
                        let new_expr = annotate_expr(*expr, &child_env);
                        for name in &patterns {
                            child_env.insert(name.clone(), expr_simple_typ(&new_expr));
                        }
                        new_stmts.push(Stmt::SLetTuple {
                            loc,
                            patterns,
                            expr: Box::new(new_expr),
                        });
                    }
                    Stmt::SLet { loc, mutable, name, expr } => {
                        let new_expr = annotate_expr(*expr, &child_env);
                        let vt = expr_simple_typ(&new_expr);
                        child_env.insert(name.clone(), vt);
                        new_stmts.push(Stmt::SLet {
                            loc,
                            mutable,
                            name,
                            expr: Box::new(new_expr),
                        });
                    }
                    Stmt::SLetTyped { loc, name, typ, expr } => {
                        let new_expr = annotate_expr(*expr, &child_env);
                        child_env.insert(name.clone(), typ.clone());
                        new_stmts.push(Stmt::SLetTyped {
                            loc,
                            name,
                            typ,
                            expr: Box::new(new_expr),
                        });
                    }
                    Stmt::SAssign { loc, name, expr } => {
                        let new_expr = annotate_expr(*expr, &child_env);
                        new_stmts.push(Stmt::SAssign {
                            loc,
                            name,
                            expr: Box::new(new_expr),
                        });
                    }
                    Stmt::SFieldAssign { loc, target, field, expr } => {
                        let new_target = annotate_expr(*target, &child_env);
                        let new_expr = annotate_expr(*expr, &child_env);
                        new_stmts.push(Stmt::SFieldAssign {
                            loc,
                            target: Box::new(new_target),
                            field,
                            expr: Box::new(new_expr),
                        });
                    }
                    Stmt::SReturn { loc, expr } => {
                        let new_expr = annotate_expr(*expr, &child_env);
                        new_stmts.push(Stmt::SReturn {
                            loc,
                            expr: Box::new(new_expr),
                        });
                    }
                    Stmt::SExpr { loc, expr } => {
                        let new_expr = annotate_expr(*expr, &child_env);
                        new_stmts.push(Stmt::SExpr {
                            loc,
                            expr: Box::new(new_expr),
                        });
                    }
                    other => new_stmts.push(other),
                }
            }
            let new_result = result.map(|r| Box::new(annotate_expr(*r, &child_env)));
            Expr::EBlock {
                loc,
                stmts: new_stmts,
                result: new_result,
            }
        }
        Expr::EArrayLit { loc, values } => {
            let new_values = values
                .into_iter()
                .map(|v| annotate_expr(v, env))
                .collect();
            Expr::EArrayLit { loc, values: new_values }
        }
        Expr::ETupleLit { loc, values } => {
            let new_values = values
                .into_iter()
                .map(|v| annotate_expr(v, env))
                .collect();
            Expr::ETupleLit { loc, values: new_values }
        }
        Expr::EStructLit { loc, name, fields, type_args } => {
            let new_fields = fields
                .into_iter()
                .map(|mut vf| {
                    vf.value = annotate_expr(vf.value, env);
                    vf
                })
                .collect();
            Expr::EStructLit { loc, name, fields: new_fields, type_args }
        }
        Expr::EAddr { loc, expr } => {
            let new_expr = annotate_expr(*expr, env);
            Expr::EAddr {
                loc,
                expr: Box::new(new_expr),
            }
        }
        Expr::EDeref { loc, expr } => {
            let new_expr = annotate_expr(*expr, env);
            Expr::EDeref {
                loc,
                expr: Box::new(new_expr),
            }
        }
        Expr::EWhile { loc, cond, body } => {
            let new_cond = annotate_expr(*cond, env);
            let new_body = annotate_expr(*body, env);
            Expr::EWhile {
                loc,
                cond: Box::new(new_cond),
                body: Box::new(new_body),
            }
        }
        Expr::ELoop { loc, body } => {
            let new_body = annotate_expr(*body, env);
            Expr::ELoop {
                loc,
                body: Box::new(new_body),
            }
        }
        Expr::EFor {
            loc,
            var,
            range,
            body,
        } => {
            let new_range = annotate_expr(*range, env);
            let mut body_env = env.clone();
            body_env.insert(var.clone(), Typ::TInt);
            let new_body = annotate_expr(*body, &body_env);
            Expr::EFor {
                loc,
                var,
                range: Box::new(new_range),
                body: Box::new(new_body),
            }
        }
        Expr::EEnumPattern { .. } => e,
        other => other,
    }
}

