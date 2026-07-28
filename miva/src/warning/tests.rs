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

fn make_func(name: &str, body: Expr, safety: Safety) -> Def {
    Def::DFunc {
        loc: loc(),
        name: name.to_string(),
        type_params: vec![],
        params: Vec::new(),
        returns: None,
        body: Box::new(body),
        safety,
        is_async: false,
        type_bounds: vec![],
    }
}

fn make_func_loc(l: Loc, name: &str, body: Expr, safety: Safety) -> Def {
    Def::DFunc {
        loc: l.clone(),
        name: name.to_string(),
        type_params: vec![],
        params: Vec::new(),
        returns: None,
        body: Box::new(body),
        safety,
        is_async: false,
        type_bounds: vec![],
    }
}

// ------------------------------------------------------------------
// Empty / no-warning cases
// ------------------------------------------------------------------

#[test]
fn test_empty_defs_no_warnings() {
    let warns = get_warnings(&[]);
    assert!(warns.is_empty());
}

#[test]
fn test_valid_program_no_warnings() {
    let defs = vec![
        make_module("test"),
        make_func("main", Expr::EVoid { loc: loc() }, Safety::Safe),
    ];
    let warns = get_warnings(&defs);
    assert!(warns.is_empty(), "Expected no warnings, got: {:?}", warns);
}

// ------------------------------------------------------------------
// W0001 – snake_case naming
// ------------------------------------------------------------------

#[test]
fn test_w0001_function_name_not_snake_case() {
    let defs = vec![
        make_module("test"),
        make_func("BadName", Expr::EVoid { loc: loc() }, Safety::Safe),
    ];
    let warns = get_warnings(&defs);
    assert_eq!(warns.len(), 1);
    assert_eq!(warns[0].code, "W0001");
    assert!(warns[0].message.contains("BadName"));
    assert!(warns[0].message.contains("function"));
}

#[test]
fn test_w0001_var_name_not_snake_case() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SLet {
                    loc: loc(),
                    mutable: false,
                    name: "BadVar".to_string(),
                    expr: Box::new(Expr::EInt {
                        loc: loc(),
                        value: 1,
                    }),
                }],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    assert_eq!(warns.len(), 1);
    assert_eq!(warns[0].code, "W0001");
    assert!(warns[0].message.contains("BadVar"));
    assert!(warns[0].message.contains("var"));
}

#[test]
fn test_w0001_module_name_not_lowercase() {
    let defs = vec![make_module("My.Mod")];
    let warns = get_warnings(&defs);
    assert_eq!(warns.len(), 1);
    assert_eq!(warns[0].code, "W0001");
    assert!(warns[0].message.contains("module"));
}

#[test]
fn test_w0001_module_name_with_uppercase() {
    let warns = get_warnings(&[make_module("Std.IO")]);
    assert!(!warns.is_empty());
    assert_eq!(warns[0].code, "W0001");
}

#[test]
fn test_module_name_with_dots_and_lowercase_is_valid() {
    let warns = get_warnings(&[make_module("std.io.utils")]);
    assert!(
        warns.is_empty(),
        "std.io.utils should be valid: {:?}",
        warns
    );
}

#[test]
fn test_snake_case_function_name_is_valid() {
    let defs = vec![
        make_module("test"),
        make_func("my_function", Expr::EVoid { loc: loc() }, Safety::Safe),
    ];
    let warns = get_warnings(&defs);
    let snake_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0001").collect();
    assert!(
        snake_warns.is_empty(),
        "my_function should pass snake check: {:?}",
        snake_warns
    );
}

#[test]
fn test_snake_case_var_name_is_valid() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SLet {
                    loc: loc(),
                    mutable: false,
                    name: "my_var".to_string(),
                    expr: Box::new(Expr::EInt {
                        loc: loc(),
                        value: 42,
                    }),
                }],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let snake_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0001").collect();
    assert!(
        snake_warns.is_empty(),
        "my_var should pass snake check: {:?}",
        snake_warns
    );
}

// ------------------------------------------------------------------
// W0002 – deprecated function calls
// ------------------------------------------------------------------

#[test]
fn test_w0002_deprecated_prints_call() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::ECall {
                loc: loc(),
                name: "prints".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    assert!(!warns.is_empty());
    assert_eq!(warns[0].code, "W0002");
    assert!(warns[0].message.contains("prints"));
    assert!(warns[0].message.contains("deprecated"));
}

#[test]
fn test_w0002_deprecated_string_concat_call() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::ECall {
                loc: loc(),
                name: "string_concat".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    assert!(!warns.is_empty());
    assert_eq!(warns[0].code, "W0002");
    assert!(warns[0].message.contains("string_concat"));
}

#[test]
fn test_no_w0002_for_deprecated_in_own_module() {
    // When inside `std.str`, string_concat should not warn
    let defs = vec![
        make_module("std.str"),
        make_func(
            "main",
            Expr::ECall {
                loc: loc(),
                name: "string_concat".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert!(
        dep_warns.is_empty(),
        "Should not warn inside std.str: {:?}",
        dep_warns
    );
}

#[test]
fn test_w0002_ptr_alloc_not_recommended() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::ECall {
                loc: loc(),
                name: "ptr_alloc".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    assert!(!warns.is_empty());
    let ptr_warn = warns.iter().find(|w| w.code == "W0002").unwrap();
    assert!(ptr_warn.message.contains("not recommended"));
}

#[test]
fn test_no_w0002_for_ptr_alloc_in_std_mem() {
    let defs = vec![
        make_module("std.mem"),
        make_func(
            "main",
            Expr::ECall {
                loc: loc(),
                name: "ptr_alloc".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let ptr_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert!(
        ptr_warns.is_empty(),
        "Should not warn inside std.mem: {:?}",
        ptr_warns
    );
}

#[test]
fn test_no_w0002_for_normal_function_call() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::ECall {
                loc: loc(),
                name: "print".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert!(
        dep_warns.is_empty(),
        "print should not trigger W0002: {:?}",
        dep_warns
    );
}

#[test]
fn test_w0002_in_nested_call_args() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::ECall {
                loc: loc(),
                name: "print".to_string(),
                type_args: vec![],
                args: vec![Expr::ECall {
                    loc: loc(),
                    name: "prints".to_string(),
                    type_args: vec![],
                    args: vec![],
                }],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    // Only the nested 'prints' should trigger W0002
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
    assert!(dep_warns[0].message.contains("prints"));
}

// ------------------------------------------------------------------
// W0003 – intro comments (bug-for-bug: always warns)
// ------------------------------------------------------------------

#[test]
fn test_w0003_cintro_always_warns() {
    let defs = vec![
        make_module("test"),
        Def::DCIntro {
            loc: loc(),
            content: "intro comment".to_string(),
        },
    ];
    let warns = get_warnings(&defs);
    // Note: DCIntro is NOT processed in the def loop (catch-all _ => ())
    // So no warning expected here. SCIntro inside a block is what triggers it.
    assert!(warns.is_empty());
}

#[test]
fn test_w0003_scintro_in_block_warns() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SCIntro {
                    loc: loc(),
                    content: "impl: some comment".to_string(),
                }],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    // SCIntro always warns (bug-for-bug)
    let cintro_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0003").collect();
    assert_eq!(cintro_warns.len(), 1);
}

#[test]
fn test_w0003_scintro_short_format() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SCIntro {
                    loc: loc(),
                    content: "no_colon".to_string(),
                }],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let cintro_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0003").collect();
    assert_eq!(cintro_warns.len(), 1);
    assert!(cintro_warns[0].message.contains("isn't valid"));
}

// ------------------------------------------------------------------
// Multiple warnings
// ------------------------------------------------------------------

#[test]
fn test_multiple_warnings() {
    let defs = vec![
        make_module("Bad.Mod"),
        make_func("BadName", Expr::EVoid { loc: loc() }, Safety::Safe),
        make_func(
            "main",
            Expr::ECall {
                loc: loc(),
                name: "prints".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    // 1 W0001 for Bad.Mod, 1 W0001 for BadName, 1 W0002 for prints
    assert_eq!(warns.len(), 3);
    assert_eq!(warns.iter().filter(|w| w.code == "W0001").count(), 2);
    assert_eq!(warns.iter().filter(|w| w.code == "W0002").count(), 1);
}

// ------------------------------------------------------------------
// Expression traversal coverage
// ------------------------------------------------------------------

#[test]
fn test_w0002_in_array_lit() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EArrayLit {
                loc: loc(),
                values: vec![Expr::ECall {
                    loc: loc(),
                    name: "string_concat".to_string(),
                    type_args: vec![],
                    args: vec![],
                }],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_bin_op() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EBinOp {
                loc: loc(),
                op: BinOp::Add,
                left: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "prints".to_string(),
                    type_args: vec![],
                    args: vec![],
                }),
                right: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 1,
                }),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_if_expr() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EIf {
                loc: loc(),
                cond: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                then: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "prints".to_string(),
                    type_args: vec![],
                    args: vec![],
                }),
                else_: Some(Box::new(Expr::ECall {
                    loc: loc(),
                    name: "printlns".to_string(),
                    type_args: vec![],
                    args: vec![],
                })),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 2);
}

#[test]
fn test_w0002_in_while_loop() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EWhile {
                loc: loc(),
                cond: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                body: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "string_concat".to_string(),
                    type_args: vec![],
                    args: vec![],
                }),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_for_loop() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EFor {
                loc: loc(),
                var: "i".to_string(),
                range: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "prints".to_string(),
                    type_args: vec![],
                    args: vec![],
                }),
                body: Box::new(Expr::EVoid { loc: loc() }),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_cast() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::ECast {
                loc: loc(),
                expr: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "prints".to_string(),
                    type_args: vec![],
                    args: vec![],
                }),
                to: Typ::TInt,
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_field_access() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EFieldAccess {
                loc: loc(),
                expr: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "prints".to_string(),
                    type_args: vec![],
                    args: vec![],
                }),
                field: "x".to_string(),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_addr() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EAddr {
                loc: loc(),
                expr: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "prints".to_string(),
                    type_args: vec![],
                    args: vec![],
                }),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_deref() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EDeref {
                loc: loc(),
                expr: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "prints".to_string(),
                    type_args: vec![],
                    args: vec![],
                }),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_choose_expr() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
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
                    then: Box::new(Expr::ECall {
                        loc: loc(),
                        name: "prints".to_string(),
                        type_args: vec![],
                        args: vec![],
                    }),
                }],
                otherwise: Some(Box::new(Expr::ECall {
                    loc: loc(),
                    name: "printlns".to_string(),
                    type_args: vec![],
                    args: vec![],
                })),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 2);
}

#[test]
fn test_w0002_in_loop() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::ELoop {
                loc: loc(),
                body: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "string_concat".to_string(),
                    type_args: vec![],
                    args: vec![],
                }),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_struct_lit_field() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EStructLit {
                loc: loc(),
                name: "Point".to_string(),
                type_args: vec![],
                fields: vec![ValueField {
                    name: "x".to_string(),
                    value: Expr::ECall {
                        loc: loc(),
                        name: "prints".to_string(),
                        type_args: vec![],
                        args: vec![],
                    },
                }],
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_return_stmt() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SReturn {
                    loc: loc(),
                    expr: Box::new(Expr::ECall {
                        loc: loc(),
                        name: "prints".to_string(),
                        type_args: vec![],
                        args: vec![],
                    }),
                }],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_expr_stmt() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SExpr {
                    loc: loc(),
                    expr: Box::new(Expr::ECall {
                        loc: loc(),
                        name: "string_length".to_string(),
                        type_args: vec![],
                        args: vec![],
                    }),
                }],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

#[test]
fn test_w0002_in_assign_stmt() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SAssign {
                    loc: loc(),
                    name: "x".to_string(),
                    expr: Box::new(Expr::ECall {
                        loc: loc(),
                        name: "prints".to_string(),
                        type_args: vec![],
                        args: vec![],
                    }),
                }],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
    assert_eq!(dep_warns.len(), 1);
}

// ------------------------------------------------------------------
// Edge cases
// ------------------------------------------------------------------

#[test]
fn test_w0002_all_deprecated_functions() {
    let deprecated = [
        "prints",
        "printlns",
        "string_concat",
        "string_parse",
        "string_length",
        "string_make",
        "ptr_alloc",
        "ptr_realloc",
        "ptr_free",
    ];
    for func_name in deprecated {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                Expr::ECall {
                    loc: loc(),
                    name: func_name.to_string(),
                    type_args: vec![],
                    args: vec![],
                },
                Safety::Safe,
            ),
        ];
        let warns = get_warnings(&defs);
        let dep_warns: Vec<_> = warns.iter().filter(|w| w.code == "W0002").collect();
        assert!(
            !dep_warns.is_empty(),
            "Expected W0002 for '{}', got: {:?}",
            func_name,
            warns
        );
    }
}

#[test]
fn test_unsafe_function_no_warning() {
    let defs = vec![
        make_module("test"),
        make_func("do_stuff", Expr::EVoid { loc: loc() }, Safety::Unsafe),
    ];
    let warns = get_warnings(&defs);
    assert!(warns.is_empty());
}

#[test]
fn test_trusted_function_no_warning() {
    let defs = vec![
        make_module("test"),
        make_func("do_stuff", Expr::EVoid { loc: loc() }, Safety::Trusted),
    ];
    let warns = get_warnings(&defs);
    assert!(warns.is_empty());
}

#[test]
fn test_dcfunccunsafe_skipped() {
    let defs = vec![
        make_module("test"),
        Def::DCFuncUnsafe {
            loc: loc(),
            name: "c_func".to_string(),
            params: Vec::new(),
            returns: None,
            code: "return 0;".to_string(),
            safety: Safety::Unsafe,
            used_c_keyword: false,
        },
    ];
    let warns = get_warnings(&defs);
    assert!(warns.is_empty());
}

#[test]
fn test_w0001_loc_correct() {
    let specific_loc = Loc { line: 42, col: 7 };
    let defs = vec![
        make_module("test"),
        make_func_loc(
            specific_loc.clone(),
            "BadName",
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ),
    ];
    let warns = get_warnings(&defs);
    assert_eq!(warns[0].loc.line, 42);
    assert_eq!(warns[0].loc.col, 7);
}

#[test]
fn test_struct_def_skipped() {
    let defs = vec![
        make_module("test"),
        Def::DStruct {
            loc: loc(),
            name: "Point".to_string(),
            type_params: vec![],
            fields: Vec::new(),
        },
    ];
    let warns = get_warnings(&defs);
    assert!(
        warns.is_empty(),
        "Struct defs should not generate warnings: {:?}",
        warns
    );
}

// ------------------------------------------------------------------
// W0003 – DCIntro annotation type checking (anoncheck.ml port)
// ------------------------------------------------------------------

fn make_dcintro(content: &str) -> Def {
    Def::DCIntro {
        loc: loc(),
        content: content.to_string(),
    }
}

fn make_safe_func(name: &str, body: Expr) -> Def {
    make_func(name, body, Safety::Safe)
}

fn make_unsafe_func(name: &str, body: Expr) -> Def {
    make_func(name, body, Safety::Unsafe)
}

fn make_trusted_func(name: &str, body: Expr) -> Def {
    make_func(name, body, Safety::Trusted)
}

fn make_struct_def(name: &str) -> Def {
    Def::DStruct {
        loc: loc(),
        name: name.to_string(),
        type_params: vec![],
        fields: Vec::new(),
    }
}

fn make_test_def(name: &str) -> Def {
    Def::DTest {
        loc: loc(),
        name: name.to_string(),
        body: Box::new(Expr::EVoid { loc: loc() }),
    }
}

fn make_cfunc_unsafe(name: &str) -> Def {
    Def::DCFuncUnsafe {
        loc: loc(),
        name: name.to_string(),
        params: Vec::new(),
        returns: None,
        code: String::new(),
        safety: Safety::Unsafe,
        used_c_keyword: false,
    }
}

fn make_export_def(symbol: &str) -> Def {
    Def::SExport {
        loc: loc(),
        symbol: symbol.to_string(),
    }
}

fn make_impl_def() -> Def {
    Def::DImpl {
        loc: loc(),
        struct_name: "Foo".to_string(),
        impls: Vec::new(),
    }
}

fn make_cmagical(content: &str) -> Def {
    Def::DCMagical {
        loc: loc(),
        content: content.to_string(),
    }
}

fn get_w3(defs: &[Def]) -> Vec<Warning> {
    get_warnings(defs)
        .into_iter()
        .filter(|w| w.code == "W0003")
        .collect()
}

// --- No DCIntro before def (no warning) ---

#[test]
fn test_dcintro_no_annotation_before_def() {
    // No DCIntro, just a function — no annotation warning
    let defs = vec![
        make_module("test"),
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "No DCIntro = no W0003: {:?}", w3);
}

// --- Safe function (DFunc) annotations ---

#[test]
fn test_dcintro_usage_before_safe_func_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("usage: used in main"),
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert!(
        w3.is_empty(),
        "usage before safe func should be valid: {:?}",
        w3
    );
}

#[test]
fn test_dcintro_param_before_safe_func_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("param: x is the input"),
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert!(
        w3.is_empty(),
        "param before safe func should be valid: {:?}",
        w3
    );
}

#[test]
fn test_dcintro_impl_before_safe_func_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("impl: this is an impl"),
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "impl before safe func should warn: {:?}", w3);
    assert!(w3[0].message.contains("invalid intro comment type"));
}

#[test]
fn test_dcintro_unsafe_before_safe_func_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("unsafe: raw memory op"),
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "unsafe before safe func should warn");
}

#[test]
fn test_dcintro_trusted_before_safe_func_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("trusted: safe wrapper"),
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "trusted before safe func should warn");
}

// --- Unsafe function (DFunc unsafe) annotations ---

#[test]
fn test_dcintro_unsafe_before_unsafe_func_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("unsafe: raw memory"),
        make_unsafe_func("do_stuff", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "unsafe before unsafe func should be valid");
}

#[test]
fn test_dcintro_usage_before_unsafe_func_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("usage: used internally"),
        make_unsafe_func("do_stuff", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "usage before unsafe func should be valid");
}

#[test]
fn test_dcintro_impl_before_unsafe_func_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("impl: something"),
        make_unsafe_func("do_stuff", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "impl before unsafe func should warn");
}

#[test]
fn test_dcintro_trusted_before_unsafe_func_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("trusted: safe wrapper"),
        make_unsafe_func("do_stuff", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "trusted before unsafe func should warn");
}

// --- Trusted function (DFunc trusted) annotations ---

#[test]
fn test_dcintro_trusted_before_trusted_func_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("trusted: verified safe"),
        make_trusted_func("trusted_op", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "trusted before trusted func should be valid");
}

#[test]
fn test_dcintro_impl_before_trusted_func_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("impl: trait impl"),
        make_trusted_func("trusted_op", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "impl before trusted func should warn");
}

#[test]
fn test_dcintro_unsafe_before_trusted_func_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("unsafe: raw"),
        make_trusted_func("trusted_op", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "unsafe before trusted func should warn");
}

// --- DCFuncUnsafe annotations ---

#[test]
fn test_dcintro_unsafe_before_cfunc_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("unsafe: C binding"),
        make_cfunc_unsafe("c_puts"),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "unsafe before cfunc should be valid");
}

#[test]
fn test_dcintro_usage_before_cfunc_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("usage: ffi call"),
        make_cfunc_unsafe("c_puts"),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "usage before cfunc should be valid");
}

#[test]
fn test_dcintro_impl_before_cfunc_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("impl: something"),
        make_cfunc_unsafe("c_puts"),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "impl before cfunc should warn");
}

// --- DTest annotations ---

#[test]
fn test_dcintro_usage_before_test_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("usage: test case"),
        make_test_def("test_foo"),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "usage before test should be valid");
}

#[test]
fn test_dcintro_impl_before_test_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("impl: trait"),
        make_test_def("test_foo"),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "impl before test should warn");
}

// --- DStruct annotations ---

#[test]
fn test_dcintro_usage_before_struct_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("usage: data type"),
        make_struct_def("Point"),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "usage before struct should be valid");
}

#[test]
fn test_dcintro_impl_before_struct_valid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("impl: trait impl for struct"),
        make_struct_def("Point"),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "impl before struct should be valid");
}

#[test]
fn test_dcintro_unsafe_before_struct_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("unsafe: raw"),
        make_struct_def("Point"),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "unsafe before struct should warn");
}

// --- Module, import, export, DImpl always warn ---

#[test]
fn test_dcintro_before_module_always_warns() {
    let defs = vec![make_dcintro("usage: module info"), make_module("test")];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "DCIntro before module should warn");
}

#[test]
fn test_dcintro_before_import_always_warns() {
    let defs = vec![
        make_dcintro("usage: import"),
        Def::SImport {
            loc: loc(),
            path: "std/io".to_string(),
        },
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "DCIntro before import should warn");
}

#[test]
fn test_dcintro_before_export_always_warns() {
    let defs = vec![make_dcintro("usage: export"), make_export_def("foo")];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "DCIntro before export should warn");
}

#[test]
fn test_dcintro_before_impl_always_warns() {
    let defs = vec![make_dcintro("usage: impl"), make_impl_def()];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "DCIntro before impl should warn");
}

// --- DCMagical and DCIntro don't need annotations ---

#[test]
fn test_dcintro_before_cmagical_no_warning() {
    let defs = vec![
        make_dcintro("usage: magical"),
        make_cmagical("warning_off W0002"),
    ];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "DCIntro before DCMagical should not warn");
}

#[test]
fn test_dcintro_before_dcintro_no_warning() {
    let defs = vec![make_dcintro("usage: first"), make_dcintro("usage: second")];
    let w3 = get_w3(&defs);
    assert!(w3.is_empty(), "DCIntro before DCIntro should not warn");
}

// --- Unknown annotation types ---

#[test]
fn test_dcintro_unknown_type_always_warns() {
    let defs = vec![
        make_module("test"),
        make_dcintro("foobar: unknown annotation"),
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "unknown annotation type should warn");
}

#[test]
fn test_dcintro_no_colon_always_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("justtext"),
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "text without colon should be invalid");
}

#[test]
fn test_dcintro_whitespace_trimmed() {
    let defs = vec![
        make_module("test"),
        make_dcintro("  usage  : some comment"),
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert!(
        w3.is_empty(),
        "whitespace-padded 'usage' should still be valid"
    );
}

// --- Multiple uses ---

#[test]
fn test_dcintro_multiple_annotations_all_valid() {
    // usage for test, usage for struct, param for func — all valid
    let defs = vec![
        make_module("test"),
        make_dcintro("usage: test case"),
        make_test_def("test_foo"),
        make_dcintro("usage: data"),
        make_struct_def("Point"),
        make_dcintro("param: x coord"),
        make_safe_func("get_x", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert!(
        w3.is_empty(),
        "all valid annotations should not warn: {:?}",
        w3
    );
}

#[test]
fn test_dcintro_multiple_annotations_one_invalid() {
    let defs = vec![
        make_module("test"),
        make_dcintro("usage: test case"),
        make_test_def("test_foo"),
        make_dcintro("unsafe: raw op"), // invalid for safe func
        make_safe_func("main", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(
        w3.len(),
        1,
        "one invalid annotation should produce one warning"
    );
}

#[test]
fn test_dcintro_invalid_before_struct_with_more_defs_after() {
    // unsafe is invalid before struct, but param is valid before safe func
    let defs = vec![
        make_module("test"),
        make_dcintro("unsafe: raw"), // invalid for struct
        make_struct_def("Point"),
        make_dcintro("param: input"), // valid for safe func
        make_safe_func("process", Expr::EVoid { loc: loc() }),
    ];
    let w3 = get_w3(&defs);
    assert_eq!(w3.len(), 1, "only struct annotation should warn");
}
