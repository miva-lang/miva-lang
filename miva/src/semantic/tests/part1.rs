use super::*;

#[test]
fn test_empty_program_has_module_error() {
    let errs = check_program(&[]);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0005"));
}

#[test]
fn test_valid_program_no_errors() {
    let defs = vec![
        make_module("test"),
        make_func("main", vec![], Expr::EVoid { loc: loc() }, Safety::Safe),
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "Expected no errors, got: {:?}", errs);
}

#[test]
fn test_use_of_undefined_variable() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EVar {
                loc: loc(),
                name: "undefined_var".to_string(),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0007"));
}

#[test]
fn test_move_then_use() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![Param::POwn {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EMove {
                            loc: loc(),
                            name: "x".to_string(),
                        }),
                    },
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EVar {
                            loc: loc(),
                            name: "x".to_string(),
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0001"));
}

#[test]
fn test_unknown_function_call() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::ECall {
                loc: loc(),
                name: "nonexistent_func".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0009"));
}

#[test]
fn test_unsafe_function_call_from_safe() {
    let defs = vec![
        make_module("test"),
        make_func(
            "dangerous",
            vec![],
            Expr::EVoid { loc: loc() },
            Safety::Unsafe,
        ),
        make_func(
            "main",
            vec![],
            Expr::ECall {
                loc: loc(),
                name: "dangerous".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0009"));
    assert!(errs.iter().any(|e| e.message.contains("unsafe function")));
}

#[test]
fn test_deref_in_safe_function() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EDeref {
                loc: loc(),
                expr: Box::new(Expr::EVoid { loc: loc() }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0010"));
}

#[test]
fn test_choose_without_otherwise() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
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
                    then: Box::new(Expr::EVoid { loc: loc() }),
                }],
                otherwise: None,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0011"));
}

#[test]
fn test_assign_to_immutable() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SLet {
                        loc: loc(),
                        mutable: false,
                        name: "x".to_string(),
                        expr: Box::new(Expr::EInt {
                            loc: loc(),
                            value: 1,
                        }),
                    },
                    Stmt::SAssign {
                        loc: loc(),
                        name: "x".to_string(),
                        expr: Box::new(Expr::EInt {
                            loc: loc(),
                            value: 2,
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0002"));
}

#[test]
fn test_is_copy_type_primitives() {
    let types = HashMap::new();
    assert!(is_copy_type(&types, &Typ::TInt));
    assert!(is_copy_type(&types, &Typ::TBool));
    assert!(is_copy_type(&types, &Typ::TFloat32));
    assert!(is_copy_type(&types, &Typ::TFloat64));
    assert!(is_copy_type(&types, &Typ::TChar));
}

#[test]
fn test_is_copy_type_non_copy() {
    let types = HashMap::new();
    assert!(!is_copy_type(&types, &Typ::TString));
    assert!(!is_copy_type(&types, &Typ::TNull));
    assert!(!is_copy_type(&types, &Typ::TPtrAny));
}

#[test]
fn test_is_copy_type_struct_all_copy() {
    let mut types = HashMap::new();
    types.insert(
        "Point".to_string(),
        vec![
            FieldDef {
                name: "x".to_string(),
                typ: Typ::TInt,
            },
            FieldDef {
                name: "y".to_string(),
                typ: Typ::TInt,
            },
        ],
    );
    assert!(is_copy_type(
        &types,
        &Typ::TStruct {
            name: "Point".to_string(),
            type_args: vec![],
            fields: vec![],
        }
    ));
}

#[test]
fn test_is_copy_type_struct_non_copy_field() {
    let mut types = HashMap::new();
    types.insert(
        "Bad".to_string(),
        vec![FieldDef {
            name: "s".to_string(),
            typ: Typ::TString,
        }],
    );
    assert!(!is_copy_type(
        &types,
        &Typ::TStruct {
            name: "Bad".to_string(),
            type_args: vec![],
            fields: vec![],
        }
    ));
}

#[test]
fn test_if_branch_move_merge() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![Param::POwn {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
            Expr::EIf {
                loc: loc(),
                cond: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                then: Box::new(Expr::EMove {
                    loc: loc(),
                    name: "x".to_string(),
                }),
                else_: Some(Box::new(Expr::EMove {
                    loc: loc(),
                    name: "x".to_string(),
                })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "Both branches move x, so x should be considered moved after if"
    );
}

#[test]
fn test_trusted_function_call_allowed() {
    let defs = vec![
        make_module("test"),
        make_func(
            "trusted_func",
            vec![],
            Expr::EVoid { loc: loc() },
            Safety::Trusted,
        ),
        make_func(
            "main",
            vec![],
            Expr::ECall {
                loc: loc(),
                name: "trusted_func".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "Trusted function calls should be allowed from safe functions"
    );
}

#[test]
fn test_safe_function_call_allowed() {
    let defs = vec![
        make_module("test"),
        make_func(
            "safe_func",
            vec![],
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ),
        make_func(
            "main",
            vec![],
            Expr::ECall {
                loc: loc(),
                name: "safe_func".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "Safe function calls should be allowed from safe functions"
    );
}

#[test]
fn test_clone_after_move_error() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![Param::POwn {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EMove {
                            loc: loc(),
                            name: "x".to_string(),
                        }),
                    },
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EClone {
                            loc: loc(),
                            name: "x".to_string(),
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0001"));
}

#[test]
fn test_ref_param_move_error() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![Param::PRef {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
            Expr::EMove {
                loc: loc(),
                name: "x".to_string(),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0002"));
}

#[test]
fn test_assign_after_move_error() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![Param::POwn {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EMove {
                            loc: loc(),
                            name: "x".to_string(),
                        }),
                    },
                    Stmt::SAssign {
                        loc: loc(),
                        name: "x".to_string(),
                        expr: Box::new(Expr::EInt {
                            loc: loc(),
                            value: 5,
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0001"));
}

#[test]
fn test_mutable_assign_resets_state() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SLet {
                        loc: loc(),
                        mutable: true,
                        name: "x".to_string(),
                        expr: Box::new(Expr::EInt {
                            loc: loc(),
                            value: 1,
                        }),
                    },
                    Stmt::SAssign {
                        loc: loc(),
                        name: "x".to_string(),
                        expr: Box::new(Expr::EInt {
                            loc: loc(),
                            value: 2,
                        }),
                    },
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EVar {
                            loc: loc(),
                            name: "x".to_string(),
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "Mutable assignment should reset state to Valid"
    );
}

#[test]
fn test_nested_block_scopes() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SLet {
                        loc: loc(),
                        mutable: false,
                        name: "outer".to_string(),
                        expr: Box::new(Expr::EInt {
                            loc: loc(),
                            value: 1,
                        }),
                    },
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EBlock {
                            loc: loc(),
                            stmts: vec![Stmt::SLet {
                                loc: loc(),
                                mutable: false,
                                name: "inner".to_string(),
                                expr: Box::new(Expr::EInt {
                                    loc: loc(),
                                    value: 2,
                                }),
                            }],
                            result: Some(Box::new(Expr::EVoid { loc: loc() })),
                        }),
                    },
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EVar {
                            loc: loc(),
                            name: "outer".to_string(),
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "Outer variable should still be accessible after nested block"
    );
}

#[test]
fn test_struct_lit_field_checking() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EStructLit {
                loc: loc(),
                name: "Point".to_string(),
                type_args: vec![],
                fields: vec![ValueField {
                    name: "x".to_string(),
                    value: Expr::EVar {
                        loc: loc(),
                        name: "undefined".to_string(),
                    },
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0007"));
}

#[test]
fn test_array_lit_element_checking() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EArrayLit {
                loc: loc(),
                values: vec![
                    Expr::EInt {
                        loc: loc(),
                        value: 1,
                    },
                    Expr::EVar {
                        loc: loc(),
                        name: "undefined".to_string(),
                    },
                ],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0007"));
}

#[test]
fn test_binop_both_sides_checked() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EBinOp {
                loc: loc(),
                op: BinOp::Add,
                left: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "left_undefined".to_string(),
                }),
                right: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "right_undefined".to_string(),
                }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.iter().filter(|e| e.code == "E0007").count() >= 2);
}

#[test]
fn test_cast_inner_checked() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::ECast {
                loc: loc(),
                expr: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "undefined".to_string(),
                }),
                to: Typ::TInt,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0007"));
}

#[test]
fn test_field_access_inner_checked() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EFieldAccess {
                loc: loc(),
                expr: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "undefined".to_string(),
                }),
                field: "x".to_string(),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0007"));
}

#[test]
fn test_while_loop_checking() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EWhile {
                loc: loc(),
                cond: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "undefined_cond".to_string(),
                }),
                body: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "undefined_body".to_string(),
                }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.iter().filter(|e| e.code == "E0007").count() >= 2);
}

#[test]
fn test_loop_body_checked() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::ELoop {
                loc: loc(),
                body: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "undefined".to_string(),
                }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0007"));
}

#[test]
fn test_for_loop_checking() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EFor {
                loc: loc(),
                var: "i".to_string(),
                range: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "undefined_range".to_string(),
                }),
                body: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "undefined_body".to_string(),
                }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.iter().filter(|e| e.code == "E0007").count() >= 2);
}

#[test]
fn test_for_loop_var_accessible_in_body() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::EFor {
                loc: loc(),
                var: "i".to_string(),
                range: Box::new(Expr::EArrayLit {
                    loc: loc(),
                    values: vec![
                        Expr::EInt {
                            loc: loc(),
                            value: 1,
                        },
                        Expr::EInt {
                            loc: loc(),
                            value: 2,
                        },
                    ],
                }),
                body: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "i".to_string(),
                }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    let var_errs: Vec<_> = errs.iter().filter(|e| e.code == "E0007").collect();
    assert!(
        var_errs.is_empty(),
        "loop variable 'i' should be accessible in body, got: {:?}",
        errs
    );
}

#[test]
fn test_multiple_module_declarations_error() {
    let defs = vec![make_module("first"), make_module("second")];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0005"));
}

#[test]
fn test_duplicate_module_declaration_error() {
    let defs = vec![
        make_module("test"),
        make_func("f", vec![], Expr::EVoid { loc: loc() }, Safety::Safe),
        make_module("test"),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0005"));
}

#[test]
fn test_invalid_magical_comment() {
    let defs = vec![
        make_module("test"),
        Def::DCMagical {
            loc: loc(),
            content: "invalid_directive".to_string(),
        },
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0013"));
}

#[test]
fn test_valid_magical_comments_no_error() {
    for directive in &[
        "warning_off foo",
        "warning_err bar",
        "release true",
        "mangle name",
    ] {
        let defs = vec![
            make_module("test"),
            Def::DCMagical {
                loc: loc(),
                content: directive.to_string(),
            },
        ];
        let errs = check_program(&defs);
        assert!(
            !errs.iter().any(|e| e.code == "E0013"),
            "Directive '{}' should be valid",
            directive
        );
    }
}
