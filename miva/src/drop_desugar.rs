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
    let mut struct_fields: HashMap<String, Vec<FieldDef>> = HashMap::new();
    let mut enum_variants: HashMap<String, Vec<EnumVariant>> = HashMap::new();
    for def in defs.iter() {
        match def {
            Def::DStruct { name, fields, .. } => {
                struct_fields.insert(name.clone(), fields.clone());
            }
            Def::DEnum { name, variants, .. } => {
                enum_variants.insert(name.clone(), variants.clone());
            }
            _ => {}
        }
    }
    // Droppability is infectious: a struct containing a droppable field or an
    // enum carrying a droppable payload (at any nesting depth) needs drop
    // glue even without its own op_drop.
    let droppable = crate::droppable::compute_droppable(defs);
    let mut fn_returns: HashMap<String, Option<Typ>> = HashMap::new();
    for def in defs.iter() {
        if let Def::DFunc { name, returns, .. } = def {
            fn_returns.insert(name.clone(), returns.clone());
        }
    }
    let ctx = Ctx {
        drop_fns: &drop_fns,
        fn_returns: &fn_returns,
        struct_fields: &struct_fields,
        enum_variants: &enum_variants,
        droppable: &droppable,
    };
    for def in defs.iter_mut() {
        match def {
            Def::DFunc { params, body, .. } => {
                let mut state = State::default();
                state.scopes.push(Vec::new());
                for p in params.iter() {
                    if let Param::POwn { name, typ } = p {
                        if let Some(t) = ctx.droppable_typ(typ) {
                            state.declare(name.clone(), t);
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
    struct_fields: &'a HashMap<String, Vec<FieldDef>>,
    enum_variants: &'a HashMap<String, Vec<EnumVariant>>,
    droppable: &'a HashSet<String>,
}

#[derive(Default)]
struct State {
    // Scope entries: Some(typ) = tracked droppable binding, None = a
    // non-droppable binding that shadows an outer name of the same name.
    scopes: Vec<Vec<(String, Option<Typ>)>>,
    types: HashMap<String, Typ>,
    moved: HashSet<String>,
    tmp_counter: usize,
}

impl State {
    fn declare(&mut self, name: String, typ: Typ) {
        self.types.insert(name.clone(), typ.clone());
        self.scopes.last_mut().unwrap().push((name, Some(typ)));
    }

    fn shadow(&mut self, name: String) {
        self.scopes.last_mut().unwrap().push((name, None));
    }

    /// True when the innermost visible binding for `name` is a non-droppable
    /// shadow, so `name` no longer refers to the tracked droppable.
    fn is_shadowed(&self, name: &str) -> bool {
        for scope in self.scopes.iter().rev() {
            for (n, t) in scope.iter().rev() {
                if n == name {
                    return t.is_none();
                }
            }
        }
        false
    }

    fn fresh_tmp(&mut self) -> String {
        self.tmp_counter += 1;
        format!("__drop_tmp{}", self.tmp_counter)
    }
}

impl<'a> Ctx<'a> {
    fn is_droppable(&self, typ: &Typ) -> bool {
        crate::droppable::is_droppable_typ(self.droppable, typ)
    }

    fn droppable_typ(&self, typ: &Typ) -> Option<Typ> {
        if self.is_droppable(typ) {
            Some(typ.clone())
        } else {
            None
        }
    }

    fn infer_droppable(&self, expr: &Expr, state: &State) -> Option<Typ> {
        match expr {
            Expr::EStructLit { name, .. } => {
                if self.droppable.contains(name) {
                    Some(Typ::TStruct {
                        name: name.clone(),
                        fields: vec![],
                        type_args: vec![],
                    })
                } else {
                    None
                }
            }
            Expr::ECall { name, .. } => {
                // Enum constructor `Enum.Variant(...)` of a droppable enum.
                if let Some((enum_name, _)) = name.rsplit_once('.') {
                    if self.enum_variants.contains_key(enum_name)
                        && self.droppable.contains(enum_name)
                    {
                        return Some(Typ::TStruct {
                            name: enum_name.to_string(),
                            fields: vec![],
                            type_args: vec![],
                        });
                    }
                }
                match self.fn_returns.get(name) {
                    Some(Some(t)) => self.droppable_typ(t),
                    _ => None,
                }
            }
            Expr::EArrayLit { values, .. } => {
                let elem = self.infer_droppable(values.first()?, state)?;
                Some(Typ::TArray { of: Box::new(elem) })
            }
            Expr::EMove { name, .. } | Expr::EVar { name, .. } | Expr::EClone { name, .. } => {
                if state.is_shadowed(name) {
                    None
                } else {
                    state.types.get(name).cloned()
                }
            }
            Expr::EBlock {
                result: Some(r), ..
            } => self.infer_droppable(r, state),
            _ => None,
        }
    }

    /// Rust destruction order: the value's own op_drop (if registered) runs
    /// first, then droppable contents recursively — struct fields in
    /// declaration order, an enum's live variant payload, array elements in
    /// index order.
    fn emit_glue(&self, loc: &Loc, base: &Expr, typ: &Typ, state: &mut State, out: &mut Vec<Stmt>) {
        match typ {
            Typ::TStruct { name, .. } => {
                if let Some(variants) = self.enum_variants.get(name) {
                    self.emit_enum_glue(loc, base, name, variants, state, out);
                    return;
                }
                if let Some(f) = self.drop_fns.get(name) {
                    out.push(Stmt::SExpr {
                        loc: loc.clone(),
                        expr: Box::new(Expr::ECall {
                            loc: loc.clone(),
                            name: f.clone(),
                            type_args: vec![],
                            args: vec![base.clone()],
                        }),
                    });
                }
                if let Some(fields) = self.struct_fields.get(name) {
                    for fd in fields {
                        if self.is_droppable(&fd.typ) {
                            let fa = Expr::EFieldAccess {
                                loc: loc.clone(),
                                expr: Box::new(base.clone()),
                                field: fd.name.clone(),
                            };
                            self.emit_glue(loc, &fa, &fd.typ, state, out);
                        }
                    }
                }
            }
            Typ::TArray { of } => {
                if !self.is_droppable(of) {
                    return;
                }
                let elem = state.fresh_tmp();
                let mut body = Vec::new();
                let elem_var = Expr::EVar {
                    loc: loc.clone(),
                    name: elem.clone(),
                };
                self.emit_glue(loc, &elem_var, of, state, &mut body);
                out.push(Stmt::SExpr {
                    loc: loc.clone(),
                    expr: Box::new(Expr::EFor {
                        loc: loc.clone(),
                        var: elem,
                        range: Box::new(base.clone()),
                        body: Box::new(Expr::EBlock {
                            loc: loc.clone(),
                            stmts: body,
                            result: None,
                        }),
                    }),
                });
            }
            _ => {}
        }
    }

    /// Enums drop only the live variant's payload: emit a `choose` whose
    /// cases bind each droppable-payload variant and drop its bindings.
    fn emit_enum_glue(
        &self,
        loc: &Loc,
        base: &Expr,
        enum_name: &str,
        variants: &[EnumVariant],
        state: &mut State,
        out: &mut Vec<Stmt>,
    ) {
        let mut cases = Vec::new();
        for v in variants {
            if !v.payload.iter().any(|t| self.is_droppable(t)) {
                continue;
            }
            let bindings: Vec<String> = v.payload.iter().map(|_| state.fresh_tmp()).collect();
            let mut body = Vec::new();
            for (b, t) in bindings.iter().zip(&v.payload) {
                if self.is_droppable(t) {
                    let bv = Expr::EVar {
                        loc: loc.clone(),
                        name: b.clone(),
                    };
                    self.emit_glue(loc, &bv, t, state, &mut body);
                }
            }
            cases.push(WhenCase {
                when: Box::new(Expr::EEnumPattern {
                    loc: loc.clone(),
                    enum_name: enum_name.to_string(),
                    variant: v.name.clone(),
                    bindings,
                }),
                guard: None,
                then: Box::new(Expr::EBlock {
                    loc: loc.clone(),
                    stmts: body,
                    result: None,
                }),
            });
        }
        if cases.is_empty() {
            return;
        }
        out.push(Stmt::SExpr {
            loc: loc.clone(),
            expr: Box::new(Expr::EChoose {
                loc: loc.clone(),
                var: Box::new(base.clone()),
                cases,
                // EVoid, not an empty block: the C++ backend deduces branch
                // types and an empty-block lambda yields `void`, clashing
                // with the unit type of the drop-call branches.
                otherwise: Some(Box::new(Expr::EVoid { loc: loc.clone() })),
            }),
        });
    }

    fn drop_stmts(&self, loc: &Loc, var: &str, typ: &Typ, state: &mut State) -> Vec<Stmt> {
        let mut out = Vec::new();
        self.emit_glue(
            loc,
            &Expr::EVar {
                loc: loc.clone(),
                name: var.to_string(),
            },
            typ,
            state,
            &mut out,
        );
        out
    }

    /// Live droppables of the innermost scope, in reverse declaration order.
    fn scope_drops(&self, state: &State, exclude: Option<&str>) -> Vec<(String, Typ)> {
        state
            .scopes
            .last()
            .unwrap()
            .iter()
            .rev()
            .filter_map(|(n, t)| t.as_ref().map(|t| (n.clone(), t.clone())))
            .filter(|(n, _)| !state.moved.contains(n) && Some(n.as_str()) != exclude)
            .collect()
    }

    /// Live droppables of ALL scopes (function exit), innermost first, each reversed.
    fn all_drops(&self, state: &State, exclude: Option<&str>) -> Vec<(String, Typ)> {
        state
            .scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .filter_map(|(n, t)| t.as_ref().map(|t| (n.clone(), t.clone())))
            .filter(|(n, _)| !state.moved.contains(n) && Some(n.as_str()) != exclude)
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
            for (n, t) in popped {
                if t.is_some() {
                    state.types.remove(&n);
                    state.moved.remove(&n);
                }
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
                    if let Some(typ) = droppable {
                        let _ = loc;
                        state.declare(name, typ);
                    } else if state.types.contains_key(&name) {
                        state.shadow(name);
                    }
                }
                Stmt::SLetTyped {
                    name, typ, expr, ..
                } => {
                    self.walk_expr(expr, state);
                    let droppable = self.droppable_typ(typ);
                    let name = name.clone();
                    out.push(stmt);
                    if let Some(typ) = droppable {
                        state.declare(name, typ);
                    } else if state.types.contains_key(&name) {
                        state.shadow(name);
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
                        for (v, typ) in &drops {
                            out.extend(self.drop_stmts(&loc, v, typ, state));
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
                        for (v, typ) in &drops {
                            out.extend(self.drop_stmts(&loc, v, typ, state));
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
                Stmt::SExpr { loc, expr } => {
                    // Statement-position `drop(x)`: expand to full drop glue
                    // (own op_drop, then droppable fields recursively).
                    let mut expanded = false;
                    if let Expr::ECall { name, args, .. } = expr.as_ref() {
                        if name == "drop" {
                            if let Some(
                                Expr::EVar { name: v, .. } | Expr::EMove { name: v, .. },
                            ) = args.first()
                            {
                                let v = v.clone();
                                if !state.is_shadowed(&v) {
                                    if let Some(typ) = state.types.get(&v).cloned() {
                                        out.extend(self.drop_stmts(loc, &v, &typ, state));
                                        state.moved.insert(v);
                                        expanded = true;
                                    }
                                }
                            }
                        }
                    }
                    if !expanded {
                        self.walk_expr(expr, state);
                        out.push(stmt);
                    }
                }
                Stmt::SAssign { expr, .. } => {
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
                    for (v, typ) in &drops {
                        out.extend(self.drop_stmts(block_loc, v, typ, state));
                    }
                }
                Some(r) if is_exit_simple(r) => {
                    for (v, typ) in &drops {
                        out.extend(self.drop_stmts(block_loc, v, typ, state));
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
                    for (v, typ) in &drops {
                        out.extend(self.drop_stmts(block_loc, v, typ, state));
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
                if !state.is_shadowed(name) {
                    state.moved.insert(name.clone());
                }
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
            Expr::ECall { name, args, .. } if name == "drop" => {
                // Builtin early-drop: `drop(x)` becomes a direct call to the
                // registered drop fn, and x is consumed (no scope-exit drop).
                if let Some(Expr::EVar { name: v, .. } | Expr::EMove { name: v, .. }) = args.first()
                {
                    let v = v.clone();
                    if state.is_shadowed(&v) {
                        return;
                    }
                    if let Some(t) = state.types.get(&v).cloned() {
                        if let Typ::TStruct { name: sn, .. } = &t {
                            if !self.enum_variants.contains_key(sn) {
                                if let Some(f) = self.drop_fns.get(sn) {
                                    *name = f.clone();
                                    if let Some(a) = args.first_mut() {
                                        if let Expr::EMove { loc, .. } = a {
                                            *a = Expr::EVar {
                                                loc: loc.clone(),
                                                name: v.clone(),
                                            };
                                        }
                                    }
                                }
                            }
                        }
                        state.moved.insert(v);
                    }
                }
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
    fn test_inner_shadowing_let_does_not_poison_outer_droppable() {
        // printlns!("...") expands to a statement block that declares its own
        // `s` and moves it; that inner `s` must not consume an outer
        // droppable also named `s`.
        let macro_block = Stmt::SExpr {
            loc: loc(),
            expr: Box::new(Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SLet {
                        loc: loc(),
                        mutable: true,
                        name: "s".to_string(),
                        expr: Box::new(Expr::EString {
                            loc: loc(),
                            value: "".to_string(),
                        }),
                    },
                    Stmt::SAssign {
                        loc: loc(),
                        name: "s".to_string(),
                        expr: Box::new(Expr::EMove {
                            loc: loc(),
                            name: "s".to_string(),
                        }),
                    },
                ],
                result: None,
            }),
        };
        let mut defs = vec![
            file_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn("main", vec![], vec![let_file("s"), macro_block], None),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        assert_eq!(
            drop_call_target(stmts.last().unwrap()),
            Some(("file_close", "s")),
            "outer droppable `s` must still be dropped at scope exit: {:?}",
            stmts
        );
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

    #[test]
    fn test_explicit_drop_call_rewritten_and_no_scope_exit_dup() {
        let mut defs = vec![
            file_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn(
                "main",
                vec![],
                vec![
                    let_file("a"),
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::ECall {
                            loc: loc(),
                            name: "drop".to_string(),
                            type_args: vec![],
                            args: vec![Expr::EVar { loc: loc(), name: "a".to_string() }],
                        }),
                    },
                ],
                None,
            ),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        // let a; drop(a) → file_close(a) ; NO extra scope-exit drop
        assert_eq!(stmts.len(), 2, "got: {:?}", stmts);
        assert_eq!(drop_call_target(&stmts[1]), Some(("file_close", "a")));
    }

    // ── recursive glue ──────────────────────────────────────────────

    fn handle_typ() -> Typ {
        Typ::TStruct {
            name: "Handle".to_string(),
            fields: vec![],
            type_args: vec![],
        }
    }

    fn handle_struct() -> Def {
        Def::DStruct {
            loc: loc(),
            name: "Handle".to_string(),
            fields: vec![FieldDef {
                name: "f".to_string(),
                typ: file_typ(),
            }],
            type_params: vec![],
        }
    }

    fn let_handle(name: &str) -> Stmt {
        Stmt::SLetTyped {
            loc: loc(),
            name: name.to_string(),
            typ: handle_typ(),
            expr: Box::new(Expr::EStructLit {
                loc: loc(),
                name: "Handle".to_string(),
                fields: vec![],
                type_args: vec![],
            }),
        }
    }

    /// Renders `SExpr(ECall(fn, [path]))` as `"fn(path)"` where path is a
    /// var / field-access chain, e.g. `"file_close(h.f)"`.
    fn call_repr(stmt: &Stmt) -> Option<String> {
        fn path_of(e: &Expr) -> Option<String> {
            match e {
                Expr::EVar { name, .. } => Some(name.clone()),
                Expr::EFieldAccess { expr, field, .. } => {
                    Some(format!("{}.{}", path_of(expr)?, field))
                }
                _ => None,
            }
        }
        if let Stmt::SExpr { expr, .. } = stmt {
            if let Expr::ECall { name, args, .. } = expr.as_ref() {
                if let Some(a) = args.first() {
                    return Some(format!("{}({})", name, path_of(a)?));
                }
            }
        }
        None
    }

    #[test]
    fn test_glue_only_struct_drops_field_at_scope_exit() {
        let mut defs = vec![
            file_struct(),
            handle_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn("main", vec![], vec![let_handle("h")], None),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        assert_eq!(stmts.len(), 2, "got: {:?}", stmts);
        assert_eq!(call_repr(&stmts[1]).as_deref(), Some("file_close(h.f)"));
    }

    #[test]
    fn test_self_drop_before_field_drop() {
        let mut defs = vec![
            file_struct(),
            handle_struct(),
            drop_impl(),
            Def::DImpl {
                loc: loc(),
                struct_name: "Handle".to_string(),
                impls: vec![ImplExpr {
                    op: ImplOp::ImDrop,
                    func: "handle_close".to_string(),
                    loc: loc(),
                }],
            },
            file_close_fn(),
            Def::DFunc {
                loc: loc(),
                name: "handle_close".to_string(),
                type_params: vec![],
                params: vec![Param::PRef {
                    name: "self".to_string(),
                    typ: handle_typ(),
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
            },
            make_fn("main", vec![], vec![let_handle("h")], None),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        assert_eq!(stmts.len(), 3, "got: {:?}", stmts);
        assert_eq!(call_repr(&stmts[1]).as_deref(), Some("handle_close(h)"));
        assert_eq!(call_repr(&stmts[2]).as_deref(), Some("file_close(h.f)"));
    }

    #[test]
    fn test_multi_level_nesting_glue_order() {
        let outer_struct = Def::DStruct {
            loc: loc(),
            name: "Outer".to_string(),
            fields: vec![FieldDef {
                name: "h".to_string(),
                typ: handle_typ(),
            }],
            type_params: vec![],
        };
        let let_outer = Stmt::SLetTyped {
            loc: loc(),
            name: "o".to_string(),
            typ: Typ::TStruct {
                name: "Outer".to_string(),
                fields: vec![],
                type_args: vec![],
            },
            expr: Box::new(Expr::EStructLit {
                loc: loc(),
                name: "Outer".to_string(),
                fields: vec![],
                type_args: vec![],
            }),
        };
        let mut defs = vec![
            file_struct(),
            handle_struct(),
            outer_struct,
            drop_impl(),
            file_close_fn(),
            make_fn("main", vec![], vec![let_outer], None),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        assert_eq!(stmts.len(), 2, "got: {:?}", stmts);
        assert_eq!(call_repr(&stmts[1]).as_deref(), Some("file_close(o.h.f)"));
    }

    #[test]
    fn test_drop_builtin_on_glue_struct_expands_to_glue() {
        let mut defs = vec![
            file_struct(),
            handle_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn(
                "main",
                vec![],
                vec![
                    let_handle("h"),
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::ECall {
                            loc: loc(),
                            name: "drop".to_string(),
                            type_args: vec![],
                            args: vec![Expr::EVar { loc: loc(), name: "h".to_string() }],
                        }),
                    },
                ],
                None,
            ),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        // let h; drop(h) → file_close(h.f) ; NO extra scope-exit drop
        assert_eq!(stmts.len(), 2, "got: {:?}", stmts);
        assert_eq!(call_repr(&stmts[1]).as_deref(), Some("file_close(h.f)"));
    }

    // ── enum / array glue ───────────────────────────────────────────

    fn slot_enum() -> Def {
        Def::DEnum {
            loc: loc(),
            name: "Slot".to_string(),
            type_params: vec![],
            variants: vec![
                EnumVariant { name: "Full".to_string(), payload: vec![file_typ()] },
                EnumVariant { name: "Empty".to_string(), payload: vec![] },
            ],
        }
    }

    fn let_slot(name: &str) -> Stmt {
        Stmt::SLetTyped {
            loc: loc(),
            name: name.to_string(),
            typ: Typ::TStruct { name: "Slot".to_string(), fields: vec![], type_args: vec![] },
            expr: Box::new(Expr::ECall {
                loc: loc(),
                name: "Slot.Full".to_string(),
                type_args: vec![],
                args: vec![Expr::EStructLit {
                    loc: loc(),
                    name: "File".to_string(),
                    fields: vec![],
                    type_args: vec![],
                }],
            }),
        }
    }

    #[test]
    fn test_enum_var_glue_drops_live_variant_payload() {
        let mut defs = vec![
            file_struct(),
            slot_enum(),
            drop_impl(),
            file_close_fn(),
            make_fn("main", vec![], vec![let_slot("s")], None),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        assert_eq!(stmts.len(), 2, "got: {:?}", stmts);
        let Stmt::SExpr { expr, .. } = &stmts[1] else {
            panic!("expected SExpr(choose), got: {:?}", stmts[1]);
        };
        let Expr::EChoose { var, cases, .. } = expr.as_ref() else {
            panic!("expected choose over enum var, got: {:?}", expr);
        };
        assert!(matches!(var.as_ref(), Expr::EVar { name, .. } if name == "s"));
        // Exactly one case: the Full variant, whose payload gets dropped.
        assert_eq!(cases.len(), 1, "got cases: {:?}", cases);
        let Expr::EEnumPattern { enum_name, variant, bindings, .. } = cases[0].when.as_ref()
        else {
            panic!("expected enum pattern, got: {:?}", cases[0].when);
        };
        assert_eq!(enum_name, "Slot");
        assert_eq!(variant, "Full");
        assert_eq!(bindings.len(), 1);
        let Expr::EBlock { stmts: case_stmts, .. } = cases[0].then.as_ref() else {
            panic!("expected block case body, got: {:?}", cases[0].then);
        };
        assert_eq!(
            call_repr(&case_stmts[0]).as_deref(),
            Some(format!("file_close({})", bindings[0]).as_str())
        );
    }

    #[test]
    fn test_array_var_glue_drops_elements_with_for_loop() {
        let mut defs = vec![
            file_struct(),
            drop_impl(),
            file_close_fn(),
            make_fn(
                "main",
                vec![],
                vec![Stmt::SLetTyped {
                    loc: loc(),
                    name: "arr".to_string(),
                    typ: Typ::TArray { of: Box::new(file_typ()) },
                    expr: Box::new(Expr::EArrayLit { loc: loc(), values: vec![] }),
                }],
                None,
            ),
        ];
        desugar_drops(&mut defs);
        let stmts = fn_body_stmts(&defs, "main");
        assert_eq!(stmts.len(), 2, "got: {:?}", stmts);
        let Stmt::SExpr { expr, .. } = &stmts[1] else {
            panic!("expected SExpr(for), got: {:?}", stmts[1]);
        };
        let Expr::EFor { var, range, body, .. } = expr.as_ref() else {
            panic!("expected for loop over array var, got: {:?}", expr);
        };
        assert!(matches!(range.as_ref(), Expr::EVar { name, .. } if name == "arr"));
        let Expr::EBlock { stmts: body_stmts, .. } = body.as_ref() else {
            panic!("expected block for body, got: {:?}", body);
        };
        assert_eq!(
            call_repr(&body_stmts[0]).as_deref(),
            Some(format!("file_close({})", var).as_str())
        );
    }
}
