// Scope-exit destructor desugaring (ADR-0001): after semantic + typecheck,
// rewrite function bodies so droppable locals get their registered op_drop
// function called at scope exits. Backends see plain function calls.

use crate::ast::*;
use std::collections::{HashMap, HashSet};

pub fn desugar_drops(defs: &mut [Def]) {
    let mut drop_fns: HashMap<String, String> = HashMap::new();
    for def in defs.iter() {
        if let Def::DImpl {
            struct_name, impls, ..
        } = def
        {
            for imp in impls {
                if matches!(imp.op, ImplOp::ImDrop) {
                    drop_fns
                        .entry(struct_name.clone())
                        .or_insert_with(|| imp.func.clone());
                }
            }
        }
    }
    if drop_fns.is_empty() {
        return;
    }
    let mut fn_returns: HashMap<String, Option<Typ>> = HashMap::new();
    for def in defs.iter() {
        if let Def::DFunc { name, returns, .. } = def {
            fn_returns.insert(name.clone(), returns.clone());
        }
    }
    let ctx = Ctx {
        drop_fns: &drop_fns,
        fn_returns: &fn_returns,
    };
    for def in defs.iter_mut() {
        match def {
            Def::DFunc { params, body, .. } => {
                let mut state = State::default();
                state.scopes.push(Vec::new());
                for p in params.iter() {
                    if let Param::POwn { name, typ } = p {
                        if let Some(sn) = ctx.droppable_struct(typ) {
                            state.declare(name.clone(), sn);
                        }
                    }
                }
                ctx.desugar_expr_as_block(body, &mut state, false);
            }
            Def::DTest { body, .. } => {
                let mut state = State::default();
                state.scopes.push(Vec::new());
                ctx.desugar_expr_as_block(body, &mut state, false);
            }
            _ => {}
        }
    }
}

struct Ctx<'a> {
    drop_fns: &'a HashMap<String, String>,
    fn_returns: &'a HashMap<String, Option<Typ>>,
}

#[derive(Default)]
struct State {
    scopes: Vec<Vec<(String, String)>>,
    types: HashMap<String, String>,
    moved: HashSet<String>,
    tmp_counter: usize,
}

impl State {
    fn declare(&mut self, name: String, struct_name: String) {
        self.types.insert(name.clone(), struct_name.clone());
        self.scopes.last_mut().unwrap().push((name, struct_name));
    }

    fn fresh_tmp(&mut self) -> String {
        self.tmp_counter += 1;
        format!("__drop_tmp{}", self.tmp_counter)
    }
}

impl<'a> Ctx<'a> {
    fn droppable_struct(&self, typ: &Typ) -> Option<String> {
        if let Typ::TStruct { name, .. } = typ {
            if self.drop_fns.contains_key(name) {
                return Some(name.clone());
            }
        }
        None
    }

    fn infer_droppable(&self, expr: &Expr, state: &State) -> Option<String> {
        match expr {
            Expr::EStructLit { name, .. } => {
                if self.drop_fns.contains_key(name) {
                    Some(name.clone())
                } else {
                    None
                }
            }
            Expr::ECall { name, .. } => match self.fn_returns.get(name) {
                Some(Some(t)) => self.droppable_struct(t),
                _ => None,
            },
            Expr::EMove { name, .. } | Expr::EVar { name, .. } | Expr::EClone { name, .. } => {
                state.types.get(name).cloned()
            }
            Expr::EBlock {
                result: Some(r), ..
            } => self.infer_droppable(r, state),
            _ => None,
        }
    }

    fn drop_stmt(&self, loc: &Loc, var: &str, struct_name: &str) -> Stmt {
        Stmt::SExpr {
            loc: loc.clone(),
            expr: Box::new(Expr::ECall {
                loc: loc.clone(),
                name: self.drop_fns[struct_name].clone(),
                type_args: vec![],
                args: vec![Expr::EVar {
                    loc: loc.clone(),
                    name: var.to_string(),
                }],
            }),
        }
    }

    /// Live droppables of the innermost scope, in reverse declaration order.
    fn scope_drops(&self, state: &State, exclude: Option<&str>) -> Vec<(String, String)> {
        state
            .scopes
            .last()
            .unwrap()
            .iter()
            .rev()
            .filter(|(n, _)| !state.moved.contains(n) && Some(n.as_str()) != exclude)
            .cloned()
            .collect()
    }

    /// Live droppables of ALL scopes (function exit), innermost first, each reversed.
    fn all_drops(&self, state: &State, exclude: Option<&str>) -> Vec<(String, String)> {
        state
            .scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .filter(|(n, _)| !state.moved.contains(n) && Some(n.as_str()) != exclude)
            .cloned()
            .collect()
    }

    fn desugar_expr_as_block(&self, expr: &mut Expr, state: &mut State, new_scope: bool) {
        if let Expr::EBlock { loc, stmts, result } = expr {
            let loc = loc.clone();
            if new_scope {
                state.scopes.push(Vec::new());
            }
            self.desugar_stmts(&loc, stmts, result, state);
            let popped = state.scopes.pop().unwrap();
            for (n, _) in popped {
                state.types.remove(&n);
                state.moved.remove(&n);
            }
            if !new_scope {
                // keep the stack balanced for the caller-seeded scope
                state.scopes.push(Vec::new());
            }
        } else {
            self.walk_expr(expr, state);
        }
    }

    fn desugar_stmts(
        &self,
        block_loc: &Loc,
        stmts: &mut Vec<Stmt>,
        result: &mut Option<Box<Expr>>,
        state: &mut State,
    ) {
        let old = std::mem::take(stmts);
        let mut out: Vec<Stmt> = Vec::with_capacity(old.len());
        for mut stmt in old {
            match &mut stmt {
                Stmt::SLet {
                    loc, name, expr, ..
                } => {
                    self.walk_expr(expr, state);
                    let droppable = self.infer_droppable(expr, state);
                    let (loc, name) = (loc.clone(), name.clone());
                    out.push(stmt);
                    if let Some(sn) = droppable {
                        let _ = loc;
                        state.declare(name, sn);
                    }
                }
                Stmt::SLetTyped {
                    name, typ, expr, ..
                } => {
                    self.walk_expr(expr, state);
                    let droppable = self.droppable_struct(typ);
                    let name = name.clone();
                    out.push(stmt);
                    if let Some(sn) = droppable {
                        state.declare(name, sn);
                    }
                }
                Stmt::SReturn { loc, expr } => {
                    self.walk_expr(expr, state);
                    let loc = loc.clone();
                    let returned = match expr.as_ref() {
                        Expr::EVar { name, .. } | Expr::EMove { name, .. } => Some(name.clone()),
                        _ => None,
                    };
                    let drops = self.all_drops(state, returned.as_deref());
                    if drops.is_empty() {
                        out.push(stmt);
                    } else if is_exit_simple(expr) {
                        for (v, sn) in &drops {
                            out.push(self.drop_stmt(&loc, v, sn));
                        }
                        out.push(stmt);
                    } else {
                        let tmp = state.fresh_tmp();
                        if let Stmt::SReturn { expr, .. } = &mut stmt {
                            let ret_expr = std::mem::replace(
                                expr.as_mut(),
                                Expr::EVar {
                                    loc: loc.clone(),
                                    name: tmp.clone(),
                                },
                            );
                            out.push(Stmt::SLet {
                                loc: loc.clone(),
                                mutable: false,
                                name: tmp.clone(),
                                expr: Box::new(ret_expr),
                            });
                        }
                        for (v, sn) in &drops {
                            out.push(self.drop_stmt(&loc, v, sn));
                        }
                        out.push(stmt);
                    }
                    for (v, _) in drops {
                        state.moved.insert(v);
                    }
                    if let Some(r) = returned {
                        state.moved.insert(r);
                    }
                }
                Stmt::SAssign { expr, .. } | Stmt::SExpr { expr, .. } => {
                    self.walk_expr(expr, state);
                    out.push(stmt);
                }
                Stmt::SFieldAssign { target, expr, .. } => {
                    self.walk_expr(target, state);
                    self.walk_expr(expr, state);
                    out.push(stmt);
                }
                _ => out.push(stmt),
            }
        }

        let result_var = match result.as_deref() {
            Some(Expr::EVar { name, .. }) | Some(Expr::EMove { name, .. }) => Some(name.clone()),
            _ => None,
        };
        if let Some(r) = result.as_deref_mut() {
            self.walk_expr(r, state);
        }
        let drops = self.scope_drops(state, result_var.as_deref());
        if !drops.is_empty() {
            match result.as_deref() {
                None => {
                    for (v, sn) in &drops {
                        out.push(self.drop_stmt(block_loc, v, sn));
                    }
                }
                Some(r) if is_exit_simple(r) => {
                    for (v, sn) in &drops {
                        out.push(self.drop_stmt(block_loc, v, sn));
                    }
                }
                Some(_) => {
                    let tmp = state.fresh_tmp();
                    let res_expr = *result.take().unwrap();
                    out.push(Stmt::SLet {
                        loc: block_loc.clone(),
                        mutable: false,
                        name: tmp.clone(),
                        expr: Box::new(res_expr),
                    });
                    for (v, sn) in &drops {
                        out.push(self.drop_stmt(block_loc, v, sn));
                    }
                    *result = Some(Box::new(Expr::EVar {
                        loc: block_loc.clone(),
                        name: tmp,
                    }));
                }
            }
        }
        *stmts = out;
    }

    fn walk_expr(&self, expr: &mut Expr, state: &mut State) {
        match expr {
            Expr::EMove { name, .. } => {
                state.moved.insert(name.clone());
            }
            Expr::EBlock { .. } => self.desugar_expr_as_block(expr, state, true),
            Expr::EIf {
                cond, then, else_, ..
            } => {
                self.walk_expr(cond, state);
                self.walk_expr(then, state);
                if let Some(e) = else_ {
                    self.walk_expr(e, state);
                }
            }
            Expr::EChoose {
                var,
                cases,
                otherwise,
                ..
            } => {
                self.walk_expr(var, state);
                for c in cases {
                    self.walk_expr(&mut c.when, state);
                    if let Some(g) = &mut c.guard {
                        self.walk_expr(g, state);
                    }
                    self.walk_expr(&mut c.then, state);
                }
                if let Some(o) = otherwise {
                    self.walk_expr(o, state);
                }
            }
            Expr::EWhile { cond, body, .. } => {
                self.walk_expr(cond, state);
                self.walk_expr(body, state);
            }
            Expr::ELoop { body, .. } => self.walk_expr(body, state),
            Expr::EFor { range, body, .. } => {
                self.walk_expr(range, state);
                self.walk_expr(body, state);
            }
            Expr::ECall { args, .. } | Expr::EMacro { args, .. } => {
                for a in args {
                    self.walk_expr(a, state);
                }
            }
            Expr::EMethodCall { expr, args, .. } => {
                self.walk_expr(expr, state);
                for a in args {
                    self.walk_expr(a, state);
                }
            }
            Expr::EBinOp { left, right, .. } => {
                self.walk_expr(left, state);
                self.walk_expr(right, state);
            }
            Expr::EFieldAccess { expr, .. }
            | Expr::ECast { expr, .. }
            | Expr::EAddr { expr, .. }
            | Expr::EDeref { expr, .. } => self.walk_expr(expr, state),
            Expr::EStructLit { fields, .. } => {
                for f in fields {
                    self.walk_expr(&mut f.value, state);
                }
            }
            Expr::EArrayLit { values, .. } => {
                for v in values {
                    self.walk_expr(v, state);
                }
            }
            Expr::ELambda { body, .. } => self.walk_expr(body, state),
            _ => {}
        }
    }
}

fn is_exit_simple(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::EVar { .. }
            | Expr::EMove { .. }
            | Expr::EInt { .. }
            | Expr::EBool { .. }
            | Expr::EFloat { .. }
            | Expr::EChar { .. }
            | Expr::EString { .. }
            | Expr::EVoid { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc() -> Loc {
        Loc { line: 1, col: 1 }
    }

    fn file_typ() -> Typ {
        Typ::TStruct {
            name: "File".to_string(),
            fields: vec![],
            type_args: vec![],
        }
    }

    fn file_struct() -> Def {
        Def::DStruct {
            loc: loc(),
            name: "File".to_string(),
            fields: vec![],
            type_params: vec![],
        }
    }

    fn drop_impl() -> Def {
        Def::DImpl {
            loc: loc(),
            struct_name: "File".to_string(),
            impls: vec![ImplExpr {
                op: ImplOp::ImDrop,
                func: "file_close".to_string(),
                loc: loc(),
            }],
        }
    }

    fn file_close_fn() -> Def {
        Def::DFunc {
            loc: loc(),
            name: "file_close".to_string(),
            type_params: vec![],
            params: vec![Param::PRef {
                name: "self".to_string(),
                typ: file_typ(),
            }],
            returns: None,
            body: Box::new(Expr::EBlock {
                loc: loc(),
                stmts: vec![],
                result: None,
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        }
    }

    fn make_fn(name: &str, params: Vec<Param>, stmts: Vec<Stmt>, result: Option<Expr>) -> Def {
        Def::DFunc {
            loc: loc(),
            name: name.to_string(),
            type_params: vec![],
            params,
            returns: None,
            body: Box::new(Expr::EBlock {
                loc: loc(),
                stmts,
                result: result.map(Box::new),
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        }
    }

    fn let_file(name: &str) -> Stmt {
        Stmt::SLet {
            loc: loc(),
            mutable: false,
            name: name.to_string(),
            expr: Box::new(Expr::EStructLit {
                loc: loc(),
                name: "File".to_string(),
                fields: vec![],
                type_args: vec![],
            }),
        }
    }

    fn fn_body_stmts<'a>(defs: &'a [Def], fname: &str) -> &'a Vec<Stmt> {
        for def in defs {
            if let Def::DFunc { name, body, .. } = def {
                if name == fname {
                    if let Expr::EBlock { stmts, .. } = body.as_ref() {
                        return stmts;
                    }
                }
            }
        }
        panic!("function {} not found", fname);
    }

    fn drop_call_target(stmt: &Stmt) -> Option<(&str, &str)> {
        if let Stmt::SExpr { expr, .. } = stmt {
            if let Expr::ECall { name, args, .. } = expr.as_ref() {
                if let Some(Expr::EVar { name: var, .. }) = args.first() {
                    return Some((name.as_str(), var.as_str()));
                }
            }
        }
        None
    }

    #[test]
    fn test_locals_dropped_in_reverse_order() {
        let mut defs = vec![
            file_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn("main", vec![], vec![let_file("a"), let_file("b")], None),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        assert_eq!(stmts.len(), 4);
        assert_eq!(drop_call_target(&stmts[2]), Some(("file_close", "b")));
        assert_eq!(drop_call_target(&stmts[3]), Some(("file_close", "a")));
    }

    #[test]
    fn test_moved_local_not_dropped() {
        let mut defs = vec![
            file_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn(
                "main",
                vec![],
                vec![
                    let_file("a"),
                    Stmt::SLet {
                        loc: loc(),
                        mutable: false,
                        name: "c".to_string(),
                        expr: Box::new(Expr::EMove {
                            loc: loc(),
                            name: "a".to_string(),
                        }),
                    },
                ],
                None,
            ),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        assert_eq!(stmts.len(), 3);
        assert_eq!(drop_call_target(&stmts[2]), Some(("file_close", "c")));
    }

    #[test]
    fn test_own_param_dropped_ref_param_not() {
        let mut defs = vec![
            file_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn(
                "consume",
                vec![
                    Param::POwn {
                        name: "f".to_string(),
                        typ: file_typ(),
                    },
                    Param::PRef {
                        name: "g".to_string(),
                        typ: file_typ(),
                    },
                ],
                vec![],
                None,
            ),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "consume");
        assert_eq!(stmts.len(), 1);
        assert_eq!(drop_call_target(&stmts[0]), Some(("file_close", "f")));
    }

    #[test]
    fn test_block_result_var_not_dropped() {
        let mut defs = vec![
            file_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn(
                "produce",
                vec![],
                vec![let_file("a"), let_file("b")],
                Some(Expr::EVar {
                    loc: loc(),
                    name: "b".to_string(),
                }),
            ),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "produce");
        assert_eq!(stmts.len(), 3);
        assert_eq!(drop_call_target(&stmts[2]), Some(("file_close", "a")));
    }

    #[test]
    fn test_return_stmt_drops_others_not_returned_value() {
        let mut defs = vec![
            file_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn(
                "produce",
                vec![],
                vec![
                    let_file("a"),
                    let_file("b"),
                    Stmt::SReturn {
                        loc: loc(),
                        expr: Box::new(Expr::EMove {
                            loc: loc(),
                            name: "b".to_string(),
                        }),
                    },
                ],
                None,
            ),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "produce");
        assert_eq!(stmts.len(), 4);
        assert_eq!(drop_call_target(&stmts[2]), Some(("file_close", "a")));
        assert!(matches!(&stmts[3], Stmt::SReturn { .. }));
    }

    #[test]
    fn test_inner_block_drops_at_block_end() {
        let inner = Expr::EBlock {
            loc: loc(),
            stmts: vec![let_file("inner")],
            result: None,
        };
        let mut defs = vec![
            file_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn(
                "main",
                vec![],
                vec![Stmt::SExpr {
                    loc: loc(),
                    expr: Box::new(inner),
                }],
                None,
            ),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        assert_eq!(stmts.len(), 1);
        if let Stmt::SExpr { expr, .. } = &stmts[0] {
            if let Expr::EBlock { stmts: inner, .. } = expr.as_ref() {
                assert_eq!(inner.len(), 2);
                assert_eq!(drop_call_target(&inner[1]), Some(("file_close", "inner")));
                return;
            }
        }
        panic!("expected inner block");
    }
}
