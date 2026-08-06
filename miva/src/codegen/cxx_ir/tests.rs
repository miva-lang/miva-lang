use super::*;

use super::*;

fn loc() -> Loc {
    Loc { line: 1, col: 1 }
}

fn call_stmt(name: &str, arg: &str) -> Stmt {
    Stmt::SExpr {
        loc: loc(),
        expr: Box::new(Expr::ECall {
            loc: loc(),
            name: name.to_string(),
            type_args: vec![],
            args: vec![Expr::EVar {
                loc: loc(),
                name: arg.to_string(),
            }],
        }),
    }
}

fn unit_fn(name: &str, stmts: Vec<Stmt>) -> Def {
    Def::DFunc {
        loc: loc(),
        name: name.to_string(),
        type_params: vec![],
        params: vec![],
        returns: None,
        body: Box::new(Expr::EBlock {
            loc: loc(),
            stmts,
            result: None,
        }),
        safety: Safety::Safe,
        is_async: false,
        type_bounds: vec![],
    }
}

#[test]
fn test_stmt_for_keeps_trailing_call_in_body() {
    let for_stmt = Stmt::SExpr {
        loc: loc(),
        expr: Box::new(Expr::EFor {
            loc: loc(),
            var: "e".to_string(),
            range: Box::new(Expr::EVar {
                loc: loc(),
                name: "arr".to_string(),
            }),
            body: Box::new(Expr::EBlock {
                loc: loc(),
                stmts: vec![call_stmt("file_close", "e")],
                result: None,
            }),
        }),
    };
    let defs = vec![unit_fn("f", vec![for_stmt])];
    let [program, _, _] = build_ir(&defs);
    assert!(
        program.contains("file_close"),
        "trailing call in for body was dropped:\n{}",
        program
    );
}

#[test]
fn test_stmt_while_keeps_trailing_call_in_body() {
    let while_stmt = Stmt::SExpr {
        loc: loc(),
        expr: Box::new(Expr::EWhile {
            loc: loc(),
            cond: Box::new(Expr::EBool {
                loc: loc(),
                value: true,
            }),
            body: Box::new(Expr::EBlock {
                loc: loc(),
                stmts: vec![call_stmt("file_close", "e")],
                result: None,
            }),
        }),
    };
    let defs = vec![unit_fn("f", vec![while_stmt])];
    let [program, _, _] = build_ir(&defs);
    assert!(
        program.contains("file_close"),
        "trailing call in while body was dropped:\n{}",
        program
    );
}

// ===== production pipeline helpers (lower → optimize → emit) =====

fn ir_expr(e: &Expr) -> String {
    let mut ctx = IrContext::new();
    emit_expr(&optimize_expr(lower_expr(&mut ctx, e)), 0, None)
}

fn ir_stmt(s: &Stmt) -> String {
    let mut ctx = IrContext::new();
    lower_stmt(&mut ctx, s)
        .into_iter()
        .flat_map(optimize_stmt)
        .map(|st| emit_stmt(&st, 0))
        .collect()
}

fn ir_def(d: &Def) -> String {
    let mut ctx = IrContext::new();
    emit_def(&optimize_def(lower_def(&mut ctx, d)), 0)
}

fn var(name: &str) -> Expr {
    Expr::EVar {
        loc: loc(),
        name: name.into(),
    }
}

fn int(value: i64) -> Expr {
    Expr::EInt { loc: loc(), value }
}

// ===== Unicode string encoding =====

#[test]
fn test_expr_string_unicode_bytes() {
    let value = "\u{2550}";
    assert_eq!(value.as_bytes(), &[0xe2, 0x95, 0x90]);

    let result = ir_expr(&Expr::EString {
        loc: loc(),
        value: value.to_string(),
    });
    let prefix = "mvp_builtin_string(\"";
    let suffix = "\")";
    assert!(result.starts_with(prefix));
    assert!(result.ends_with(suffix));
    let inner = &result[prefix.len()..result.len() - suffix.len()];
    assert_eq!(
        inner.as_bytes(),
        &[0xe2, 0x95, 0x90],
        "Inner string value should be correct UTF-8, not C3 A2 C2 95 C2 90"
    );
}

#[test]
fn test_build_ir_unicode_with_macro_expansion() {
    let body_expr = Expr::EBlock {
        loc: loc(),
        stmts: vec![Stmt::SExpr {
            loc: loc(),
            expr: Box::new(Expr::ECall {
                loc: loc(),
                name: "print".to_string(),
                type_args: vec![],
                args: vec![Expr::EString {
                    loc: loc(),
                    value: "\u{2550}\u{2550}\u{2550} Hello\u{2550}\u{2550}\u{2550}".to_string(),
                }],
            }),
        }],
        result: None,
    };

    let defs = vec![Def::DFunc {
        loc: loc(),
        name: "main".to_string(),
        type_params: vec![],
        params: vec![],
        returns: None,
        body: Box::new(body_expr),
        safety: Safety::Safe,
        is_async: false,
        type_bounds: vec![],
    }];

    let [program, _header, _test] = build_ir(&defs);

    let correct: &[u8] = &[0xe2, 0x95, 0x90];
    let wrong_triple: &[u8] = &[0xc3, 0xa2, 0xc2, 0x95, 0xc2, 0x90];

    assert!(
        program.as_bytes().windows(3).any(|w| w == correct),
        "program bytes should contain correct UTF-8 for \u{2550}"
    );
    assert!(
        !program.as_bytes().windows(6).any(|w| w == wrong_triple),
        "program bytes should NOT contain double-encoded UTF-8"
    );
}

// ===== expr - primitives =====

#[test]
fn test_expr_int() {
    assert_eq!(ir_expr(&int(42)), "static_cast<mvp_builtin_int>(42)");
}

#[test]
fn test_expr_neg_int() {
    assert_eq!(ir_expr(&int(-5)), "static_cast<mvp_builtin_int>(-5)");
}

#[test]
fn test_expr_bool_true() {
    let e = Expr::EBool {
        loc: loc(),
        value: true,
    };
    assert_eq!(ir_expr(&e), "mvp_builtin_boolean(true)");
}

#[test]
fn test_expr_bool_false() {
    let e = Expr::EBool {
        loc: loc(),
        value: false,
    };
    assert_eq!(ir_expr(&e), "mvp_builtin_boolean(false)");
}

#[test]
fn test_expr_float() {
    let e = Expr::EFloat {
        loc: loc(),
        value: 3.14,
    };
    assert_eq!(ir_expr(&e), "mvp_builtin_float(3.14)");
}

#[test]
fn test_expr_float_zero() {
    let e = Expr::EFloat {
        loc: loc(),
        value: 0.0,
    };
    assert_eq!(ir_expr(&e), "mvp_builtin_float(0)");
}

#[test]
fn test_expr_char() {
    let e = Expr::EChar {
        loc: loc(),
        value: "a".into(),
    };
    assert_eq!(ir_expr(&e), "mvp_builtin_byte('a')");
}

#[test]
fn test_expr_string() {
    let e = Expr::EString {
        loc: loc(),
        value: "hello".into(),
    };
    assert_eq!(ir_expr(&e), "mvp_builtin_string(\"hello\")");
}

#[test]
fn test_expr_var() {
    assert_eq!(ir_expr(&var("x")), "x");
}

#[test]
fn test_expr_move() {
    let e = Expr::EMove {
        loc: loc(),
        name: "x".into(),
    };
    assert_eq!(ir_expr(&e), "std::move(x)");
}

#[test]
fn test_expr_clone() {
    let e = Expr::EClone {
        loc: loc(),
        name: "x".into(),
    };
    assert_eq!(ir_expr(&e), "decltype(x)(x)");
}

#[test]
fn test_expr_void() {
    assert_eq!(ir_expr(&Expr::EVoid { loc: loc() }), "mvp_builtin_void");
}

#[test]
fn test_expr_addr() {
    let e = Expr::EAddr {
        loc: loc(),
        expr: Box::new(var("x")),
    };
    assert_eq!(ir_expr(&e), "&(x)");
}

#[test]
fn test_expr_deref() {
    let e = Expr::EDeref {
        loc: loc(),
        expr: Box::new(var("p")),
    };
    assert_eq!(ir_expr(&e), "*(p)");
}

#[test]
fn test_expr_macro_empty() {
    let e = Expr::EMacro {
        loc: loc(),
        name: "something".into(),
        args: vec![],
    };
    assert_eq!(ir_expr(&e), "");
}

#[test]
fn test_expr_field_access() {
    let e = Expr::EFieldAccess {
        loc: loc(),
        expr: Box::new(var("p")),
        field: "x".into(),
    };
    assert_eq!(ir_expr(&e), "p.x");
}

#[test]
fn test_expr_cast() {
    let e = Expr::ECast {
        loc: loc(),
        expr: Box::new(int(65)),
        to: Typ::TChar,
    };
    assert_eq!(
        ir_expr(&e),
        "static_cast<mvp_builtin_byte>(static_cast<mvp_builtin_int>(65))"
    );
}

// ===== binop =====

fn binop(op: BinOp) -> Expr {
    Expr::EBinOp {
        loc: loc(),
        op,
        left: Box::new(var("a")),
        right: Box::new(var("b")),
    }
}

#[test]
fn test_binop_add() {
    assert_eq!(ir_expr(&binop(BinOp::Add)), "(a + b)");
}

#[test]
fn test_binop_sub() {
    assert_eq!(ir_expr(&binop(BinOp::Sub)), "(a - b)");
}

#[test]
fn test_binop_mul() {
    assert_eq!(ir_expr(&binop(BinOp::Mul)), "(a * b)");
}

#[test]
fn test_binop_eq() {
    assert_eq!(ir_expr(&binop(BinOp::Eq)), "(a == b)");
}

#[test]
fn test_binop_neq() {
    assert_eq!(ir_expr(&binop(BinOp::Neq)), "(a != b)");
}

#[test]
fn test_binop_constant_folding() {
    let e = Expr::EBinOp {
        loc: loc(),
        op: BinOp::Add,
        left: Box::new(int(1)),
        right: Box::new(int(2)),
    };
    assert_eq!(ir_expr(&e), "static_cast<mvp_builtin_int>(3)");
}

// ===== call =====

fn call(name: &str, type_args: Vec<Typ>, args: Vec<Expr>) -> Expr {
    Expr::ECall {
        loc: loc(),
        name: name.into(),
        type_args,
        args,
    }
}

#[test]
fn test_call_no_args() {
    assert_eq!(ir_expr(&call("foo", vec![], vec![])), "foo()");
}

#[test]
fn test_call_with_args() {
    let e = call("add", vec![], vec![var("x"), int(1)]);
    assert_eq!(ir_expr(&e), "add(x, static_cast<mvp_builtin_int>(1))");
}

#[test]
fn test_call_builtin_print() {
    let e = call(
        "print",
        vec![],
        vec![Expr::EString {
            loc: loc(),
            value: "hello".into(),
        }],
    );
    assert_eq!(ir_expr(&e), "mvp_print(mvp_builtin_string(\"hello\"))");
}

// ===== if =====

#[test]
fn test_if_no_else() {
    let e = Expr::EIf {
        loc: loc(),
        cond: Box::new(var("c")),
        then: Box::new(int(1)),
        else_: None,
    };
    let result = ir_expr(&e);
    assert!(
        result.starts_with("([&]() -> void { if ("),
        "got: {}",
        result
    );
    assert!(result.contains("c"));
    assert!(result.contains("1"));
}

#[test]
fn test_if_with_else() {
    let e = Expr::EIf {
        loc: loc(),
        cond: Box::new(var("c")),
        then: Box::new(int(1)),
        else_: Some(Box::new(int(2))),
    };
    assert!(ir_expr(&e).contains("else"));
}

#[test]
fn test_if_constant_cond_folds_to_then() {
    let e = Expr::EIf {
        loc: loc(),
        cond: Box::new(Expr::EBool {
            loc: loc(),
            value: true,
        }),
        then: Box::new(int(1)),
        else_: Some(Box::new(int(2))),
    };
    assert_eq!(ir_expr(&e), "static_cast<mvp_builtin_int>(1)");
}

#[test]
fn test_choose_with_guard() {
    let e = Expr::EChoose {
        loc: loc(),
        var: Box::new(var("x")),
        cases: vec![WhenCase {
            when: Box::new(int(1)),
            guard: Some(Box::new(Expr::EBinOp {
                loc: loc(),
                op: BinOp::Gt,
                left: Box::new(var("x")),
                right: Box::new(int(0)),
            })),
            then: Box::new(int(10)),
        }],
        otherwise: Some(Box::new(int(0))),
    };
    let result = ir_expr(&e);
    assert!(result.contains("&&"), "expected guard && in: {}", result);
    assert!(
        result.contains("x >"),
        "expected guard comparison in: {}",
        result
    );
}

// ===== loops in value position =====

#[test]
fn test_while_basic() {
    let e = Expr::EWhile {
        loc: loc(),
        cond: Box::new(Expr::EBool {
            loc: loc(),
            value: true,
        }),
        body: Box::new(Expr::EVoid { loc: loc() }),
    };
    assert!(ir_expr(&e).starts_with("([&]() { while ("));
}

#[test]
fn test_loop_basic() {
    let e = Expr::ELoop {
        loc: loc(),
        body: Box::new(Expr::EVoid { loc: loc() }),
    };
    assert!(ir_expr(&e).starts_with("([&]() { for (;;) {"));
}

#[test]
fn test_for_basic() {
    let e = Expr::EFor {
        loc: loc(),
        var: "i".into(),
        range: Box::new(var("range")),
        body: Box::new(Expr::EVoid { loc: loc() }),
    };
    assert!(ir_expr(&e).starts_with("([&]() { for (const auto& i : range) {"));
}

// ===== array literal =====

#[test]
fn test_array_lit_empty() {
    let e = Expr::EArrayLit {
        loc: loc(),
        values: vec![],
    };
    assert_eq!(ir_expr(&e), "std::vector{}");
}

#[test]
fn test_array_lit_values() {
    let e = Expr::EArrayLit {
        loc: loc(),
        values: vec![int(1), int(2)],
    };
    assert_eq!(
        ir_expr(&e),
        "std::vector{static_cast<mvp_builtin_int>(1), static_cast<mvp_builtin_int>(2)}"
    );
}

// ===== stmt =====

#[test]
fn test_stmt_let_mutable() {
    let stmt = Stmt::SLet {
        loc: loc(),
        mutable: true,
        name: "x".into(),
        expr: Box::new(int(5)),
    };
    assert_eq!(
        ir_stmt(&stmt),
        "auto x = static_cast<mvp_builtin_int>(5);\n"
    );
}

#[test]
fn test_stmt_let_immutable() {
    let stmt = Stmt::SLet {
        loc: loc(),
        mutable: false,
        name: "x".into(),
        expr: Box::new(int(5)),
    };
    assert_eq!(
        ir_stmt(&stmt),
        "const auto x = static_cast<mvp_builtin_int>(5);\n"
    );
}

#[test]
fn test_stmt_return() {
    let stmt = Stmt::SReturn {
        loc: loc(),
        expr: Box::new(int(0)),
    };
    assert_eq!(ir_stmt(&stmt), "return static_cast<mvp_builtin_int>(0);\n");
}

#[test]
fn test_stmt_expr() {
    let stmt = Stmt::SExpr {
        loc: loc(),
        expr: Box::new(call(
            "print",
            vec![],
            vec![Expr::EString {
                loc: loc(),
                value: "hi".into(),
            }],
        )),
    };
    assert_eq!(ir_stmt(&stmt), "mvp_print(mvp_builtin_string(\"hi\"));\n");
}

#[test]
fn test_stmt_assign() {
    let stmt = Stmt::SAssign {
        loc: loc(),
        name: "x".into(),
        expr: Box::new(int(10)),
    };
    assert_eq!(ir_stmt(&stmt), "x = static_cast<mvp_builtin_int>(10);\n");
}

#[test]
fn test_stmt_let_typed() {
    let stmt = Stmt::SLetTyped {
        loc: loc(),
        name: "x".into(),
        typ: Typ::TInt,
        expr: Box::new(int(5)),
    };
    assert_eq!(
        ir_stmt(&stmt),
        "mvp_builtin_int x = static_cast<mvp_builtin_int>(5);\n"
    );
}

// ===== enum defs =====

#[test]
fn test_enum_def() {
    let def = Def::DEnum {
        loc: loc(),
        name: "Color".into(),
        variants: vec![
            EnumVariant {
                name: "Red".into(),
                payload: vec![],
            },
            EnumVariant {
                name: "Green".into(),
                payload: vec![Typ::TInt],
            },
        ],
        type_params: vec![],
    };
    let out = ir_def(&def);
    assert!(
        out.contains("struct Color"),
        "expected struct Color in:\n{}",
        out
    );
    assert!(out.contains("__tag"), "expected __tag in:\n{}", out);
    assert!(
        out.contains("Color_Green("),
        "expected Color_Green ctor in:\n{}",
        out
    );
}

#[test]
fn test_generic_enum_def() {
    let def = Def::DEnum {
        loc: loc(),
        name: "Box".into(),
        variants: vec![
            EnumVariant {
                name: "Value".into(),
                payload: vec![Typ::TGenericParam { name: "T".into() }],
            },
            EnumVariant {
                name: "Empty".into(),
                payload: vec![],
            },
        ],
        type_params: vec!["T".into()],
    };
    let out = ir_def(&def);
    assert!(
        out.contains("template<typename T>"),
        "expected template header in:\n{}",
        out
    );
    assert!(
        out.contains("struct Box"),
        "expected struct Box in:\n{}",
        out
    );
    assert!(
        out.contains("inline Box<T> Box_Value(T __a0)"),
        "expected generic ctor in:\n{}",
        out
    );
    assert!(
        out.contains("Box_Value_tag()"),
        "expected tag accessor in:\n{}",
        out
    );
}

#[test]
fn test_generic_enum_ctor_call() {
    let e = call("Value", vec![Typ::TInt], vec![var("Box"), int(5)]);
    assert_eq!(
        ir_expr(&e),
        "Box_Value<mvp_builtin_int>(static_cast<mvp_builtin_int>(5))"
    );
}
