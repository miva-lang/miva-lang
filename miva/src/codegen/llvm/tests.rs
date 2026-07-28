use super::*;

use super::*;

fn loc() -> Loc {
    Loc { line: 1, col: 1 }
}

fn int(v: i64) -> Expr {
    Expr::EInt { loc: loc(), value: v }
}

#[test]
fn test_inner_block_let_does_not_clobber_outer_binding() {
    let mut ctx = LlvmCtx::new();
    let mut body = String::new();
    gen_stmt(
        &Stmt::SLet { loc: loc(), mutable: false, name: "s".into(), expr: Box::new(int(1)) },
        &mut ctx,
        &mut body,
    );
    let outer = ctx.get_var_reload("s");

    let inner_block = Expr::EBlock {
        loc: loc(),
        stmts: vec![
            Stmt::SLet { loc: loc(), mutable: true, name: "s".into(), expr: Box::new(int(2)) },
            Stmt::SAssign { loc: loc(), name: "s".into(), expr: Box::new(int(3)) },
        ],
        result: None,
    };
    gen_stmt(
        &Stmt::SExpr { loc: loc(), expr: Box::new(inner_block) },
        &mut ctx,
        &mut body,
    );

    assert_eq!(
        ctx.get_var_reload("s"),
        outer,
        "a let inside a nested block must not clobber the outer binding"
    );
}

#[test]
fn test_for_over_array_literal_iterates_elements() {
    let mut ctx = LlvmCtx::new();
    let mut body = String::new();
    gen_stmt(
        &Stmt::SLet {
            loc: loc(),
            mutable: false,
            name: "arr".into(),
            expr: Box::new(Expr::EArrayLit { loc: loc(), values: vec![int(7), int(8)] }),
        },
        &mut ctx,
        &mut body,
    );
    let for_expr = Expr::EFor {
        loc: loc(),
        var: "x".into(),
        range: Box::new(Expr::EVar { loc: loc(), name: "arr".into() }),
        body: Box::new(Expr::EBlock { loc: loc(), stmts: vec![], result: None }),
    };
    gen_expr(&for_expr, &mut ctx, &mut body);

    assert!(
        body.contains("call ptr @miva_alloc(i64 24)"),
        "array literal should heap-allocate len + 2 element slots:\n{}",
        body
    );
    assert!(
        body.contains("store i64 2, ptr"),
        "array literal should store its length at slot 0:\n{}",
        body
    );
    assert!(
        body.contains("%fel"),
        "for-over-array should load each element into the loop var:\n{}",
        body
    );
}
