use super::*;

#[test]
fn test_magical_comment_too_short() {
    let defs = vec![
        make_module("test"),
        Def::DCMagical {
            loc: loc(),
            content: "warning_off".to_string(),
        },
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0013"));
}

#[test]
fn test_test_def_checks_body() {
    let defs = vec![
        make_module("test"),
        make_test_def(
            "test_foo",
            Expr::EVar {
                loc: loc(),
                name: "undefined_in_test".to_string(),
            },
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0007"));
}

#[test]
fn test_call_with_args_checks_each_arg() {
    let defs = vec![
        make_module("test"),
        make_func("helper", vec![], Expr::EVoid { loc: loc() }, Safety::Safe),
        make_func(
            "main",
            vec![],
            Expr::ECall {
                loc: loc(),
                name: "helper".to_string(),
                type_args: vec![],
                args: vec![
                    Expr::EVar {
                        loc: loc(),
                        name: "undefined1".to_string(),
                    },
                    Expr::EVar {
                        loc: loc(),
                        name: "undefined2".to_string(),
                    },
                ],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.iter().filter(|e| e.code == "E0007").count() >= 2);
}

#[test]
fn test_builtin_function_call_known() {
    for builtin in &[
        "print",
        "println",
        "string_concat",
        "box_new",
        "range",
        "exit",
        "abort",
        "panic",
    ] {
        let defs = vec![
            make_module("test"),
            make_func(
                "main",
                vec![],
                Expr::ECall {
                    loc: loc(),
                    name: builtin.to_string(),
                    type_args: vec![],
                    args: vec![],
                },
                Safety::Safe,
            ),
        ];
        let errs = check_program(&defs);
        let unknown_errs: Vec<_> = errs
            .iter()
            .filter(|e| e.code == "E0009" && e.message.contains("unknown function"))
            .collect();
        assert!(
            unknown_errs.is_empty(),
            "builtin '{}' should not be unknown, errors: {:?}",
            builtin,
            errs
        );
    }
}

#[test]
fn test_builtin_ptr_set_unsafe() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Expr::ECall {
                loc: loc(),
                name: "ptr_set".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs
        .iter()
        .any(|e| e.code == "E0009" && e.message.contains("unsafe function")));
}

#[test]
fn test_user_function_overrides_builtin() {
    let defs = vec![
        make_module("test"),
        make_func("print", vec![], Expr::EVoid { loc: loc() }, Safety::Safe),
        make_func(
            "main",
            vec![],
            Expr::ECall {
                loc: loc(),
                name: "print".to_string(),
                type_args: vec![],
                args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    let unknown_errs: Vec<_> = errs
        .iter()
        .filter(|e| e.code == "E0009" && e.message.contains("unknown function"))
        .collect();
    assert!(
         unknown_errs.is_empty(),
         "user-defined 'print' should override builtin, errors: {:?}",
         errs
     );
}

#[test]
fn test_clone_prevents_move_error() {
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
                    Stmt::SLet {
                        loc: loc(),
                        mutable: true,
                        name: "y".to_string(),
                        expr: Box::new(Expr::EClone {
                            loc: loc(),
                            name: "x".to_string(),
                        }),
                    },
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
                            name: "y".to_string(),
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "clone should prevent move error, got: {:?}", errs);
}

#[test]
fn test_multiple_unsafe_calls_from_safe() {
    let defs = vec![
        make_module("test"),
        make_func("unsafe_fn1", vec![], Expr::EVoid { loc: loc() }, Safety::Unsafe),
        make_func("unsafe_fn2", vec![], Expr::EVoid { loc: loc() }, Safety::Unsafe),
        make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::ECall {
                            loc: loc(),
                            name: "unsafe_fn1".to_string(),
                            type_args: vec![],
                            args: vec![],
                        }),
                    },
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::ECall {
                            loc: loc(),
                            name: "unsafe_fn2".to_string(),
                            type_args: vec![],
                            args: vec![],
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0009" && e.message.contains("unsafe function")));
}

#[test]
fn test_nested_block_scoping() {
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
                        expr: Box::new(Expr::EInt { loc: loc(), value: 1 }),
                    },
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EBlock {
                            loc: loc(),
                            stmts: vec![Stmt::SLet {
                                loc: loc(),
                                mutable: false,
                                name: "x".to_string(),
                                expr: Box::new(Expr::EInt { loc: loc(), value: 2 }),
                            }],
                            result: Some(Box::new(Expr::EVar {
                                loc: loc(),
                                name: "x".to_string(),
                            })),
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EVar {
                    loc: loc(),
                    name: "x".to_string(),
                })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "nested block should shadow outer var, got: {:?}", errs);
}

#[test]
fn test_move_in_different_branches() {
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
                cond: Box::new(Expr::EBool { loc: loc(), value: true }),
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
    assert!(errs.is_empty(), "move in both branches should be ok, got: {:?}", errs);
}

#[test]
fn test_struct_field_access_valid() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "Point",
            vec![
                FieldDef { name: "x".to_string(), typ: Typ::TInt },
                FieldDef { name: "y".to_string(), typ: Typ::TInt },
            ],
        ),
        make_func(
            "main",
            vec![],
            Expr::EFieldAccess {
                loc: loc(),
                expr: Box::new(Expr::EStructLit {
                    loc: loc(),
                    name: "Point".to_string(),
                    fields: vec![
                        ValueField {
                            name: "x".to_string(),
                            value: Expr::EInt { loc: loc(), value: 1 },
                        },
                        ValueField {
                            name: "y".to_string(),
                            value: Expr::EInt { loc: loc(), value: 2 },
                        },
                    ],
                    type_args: vec![],
                }),
                field: "x".to_string(),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "valid field access should have no errors, got: {:?}", errs);
}

// ── droppable move-only enforcement ─────────────────────────────

fn file_typ() -> Typ {
    Typ::TStruct {
        name: "File".to_string(),
        fields: vec![],
        type_args: vec![],
    }
}

fn file_lit() -> Expr {
    Expr::EStructLit {
        loc: loc(),
        name: "File".to_string(),
        fields: vec![],
        type_args: vec![],
    }
}

fn drop_defs() -> Vec<Def> {
    vec![
        make_module("test"),
        make_struct("File", vec![FieldDef { name: "id".to_string(), typ: Typ::TInt }]),
        Def::DImpl {
            loc: loc(),
            struct_name: "File".to_string(),
            impls: vec![ImplExpr {
                op: ImplOp::ImDrop,
                func: "file_close".to_string(),
                loc: loc(),
            }],
        },
        make_func(
            "file_close",
            vec![Param::PRef { name: "self".to_string(), typ: file_typ() }],
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ),
    ]
}

fn let_file(name: &str) -> Stmt {
    Stmt::SLetTyped {
        loc: loc(),
        name: name.to_string(),
        typ: file_typ(),
        expr: Box::new(file_lit()),
    }
}

fn use_var(name: &str) -> Stmt {
    Stmt::SExpr {
        loc: loc(),
        expr: Box::new(Expr::EVar {
            loc: loc(),
            name: name.to_string(),
        }),
    }
}

#[test]
fn test_droppable_second_use_after_implicit_move_errors() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "b".to_string(),
                    typ: file_typ(),
                    expr: Box::new(Expr::EVar { loc: loc(), name: "a".to_string() }),
                },
                use_var("a"),
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0001"), "expected E0001, got: {:?}", errs);
}

#[test]
fn test_droppable_clone_allowed() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "b".to_string(),
                    typ: file_typ(),
                    expr: Box::new(Expr::EClone { loc: loc(), name: "a".to_string() }),
                },
                use_var("a"),
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "clone of droppable should be allowed, got: {:?}", errs);
}

#[test]
fn test_droppable_call_arg_moves_ref_arg_does_not() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "consume",
        vec![Param::POwn { name: "f".to_string(), typ: file_typ() }],
        Expr::EVoid { loc: loc() },
        Safety::Safe,
    ));
    defs.push(make_func(
        "inspect",
        vec![Param::PRef { name: "f".to_string(), typ: file_typ() }],
        Expr::EVoid { loc: loc() },
        Safety::Safe,
    ));
    // ref arg does not move: inspect(a); use a → ok
    defs.push(make_func(
        "ok_fn",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SExpr {
                    loc: loc(),
                    expr: Box::new(Expr::ECall {
                        loc: loc(),
                        name: "inspect".to_string(),
                        type_args: vec![],
                        args: vec![Expr::EVar { loc: loc(), name: "a".to_string() }],
                    }),
                },
                use_var("a"),
            ],
            result: None,
        },
        Safety::Safe,
    ));
    // own arg moves: consume(b); use b → E0001
    defs.push(make_func(
        "bad_fn",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("b"),
                Stmt::SExpr {
                    loc: loc(),
                    expr: Box::new(Expr::ECall {
                        loc: loc(),
                        name: "consume".to_string(),
                        type_args: vec![],
                        args: vec![Expr::EVar { loc: loc(), name: "b".to_string() }],
                    }),
                },
                use_var("b"),
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0001"), "expected E0001 for own-arg reuse, got: {:?}", errs);
    assert_eq!(errs.iter().filter(|e| e.code == "E0001").count(), 1, "ref arg must not move, got: {:?}", errs);
}

#[test]
fn test_droppable_branch_inconsistent_move_errors() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SExpr {
                    loc: loc(),
                    expr: Box::new(Expr::EIf {
                        loc: loc(),
                        cond: Box::new(Expr::EBool { loc: loc(), value: true }),
                        then: Box::new(Expr::EBlock {
                            loc: loc(),
                            stmts: vec![Stmt::SExpr {
                                loc: loc(),
                                expr: Box::new(Expr::EMove { loc: loc(), name: "a".to_string() }),
                            }],
                            result: None,
                        }),
                        else_: None,
                    }),
                },
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0033"), "expected E0033, got: {:?}", errs);
}

#[test]
fn test_droppable_both_branches_move_ok() {
    let move_a_block = Expr::EBlock {
        loc: loc(),
        stmts: vec![Stmt::SExpr {
            loc: loc(),
            expr: Box::new(Expr::EMove { loc: loc(), name: "a".to_string() }),
        }],
        result: None,
    };
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SExpr {
                    loc: loc(),
                    expr: Box::new(Expr::EIf {
                        loc: loc(),
                        cond: Box::new(Expr::EBool { loc: loc(), value: true }),
                        then: Box::new(move_a_block.clone()),
                        else_: Some(Box::new(move_a_block)),
                    }),
                },
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(!errs.iter().any(|e| e.code == "E0033"), "both branches move is legal, got: {:?}", errs);
}

#[test]
fn test_non_droppable_struct_second_use_still_allowed() {
    let defs = vec![
        make_module("test"),
        make_struct("Point", vec![FieldDef { name: "x".to_string(), typ: Typ::TInt }]),
        make_func(
            "main",
            vec![],
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SLetTyped {
                        loc: loc(),
                        name: "p".to_string(),
                        typ: Typ::TStruct {
                            name: "Point".to_string(),
                            fields: vec![],
                            type_args: vec![],
                        },
                        expr: Box::new(Expr::EStructLit {
                            loc: loc(),
                            name: "Point".to_string(),
                            fields: vec![ValueField {
                                name: "x".to_string(),
                                value: Expr::EInt { loc: loc(), value: 1 },
                            }],
                            type_args: vec![],
                        }),
                    },
                    Stmt::SLetTyped {
                        loc: loc(),
                        name: "q".to_string(),
                        typ: Typ::TStruct {
                            name: "Point".to_string(),
                            fields: vec![],
                            type_args: vec![],
                        },
                        expr: Box::new(Expr::EVar { loc: loc(), name: "p".to_string() }),
                    },
                    use_var("p"),
                ],
                result: None,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "non-droppable struct copy must stay legal, got: {:?}", errs);
}

// ── builtin drop(x) ─────────────────────────────────────────────

fn drop_call(var: &str) -> Stmt {
    Stmt::SExpr {
        loc: loc(),
        expr: Box::new(Expr::ECall {
            loc: loc(),
            name: "drop".to_string(),
            type_args: vec![],
            args: vec![Expr::EVar { loc: loc(), name: var.to_string() }],
        }),
    }
}

#[test]
fn test_drop_builtin_marks_var_moved() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![let_file("a"), drop_call("a"), use_var("a")],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0001"), "use after drop(a) should be E0001, got: {:?}", errs);
}

#[test]
fn test_drop_builtin_valid_use_no_errors() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![let_file("a"), drop_call("a")],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "valid drop(a) should have no errors, got: {:?}", errs);
}

#[test]
fn test_drop_builtin_on_non_droppable_is_e0035() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                Stmt::SLet {
                    loc: loc(),
                    mutable: false,
                    name: "n".to_string(),
                    expr: Box::new(Expr::EInt { loc: loc(), value: 1 }),
                },
                drop_call("n"),
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0035"), "drop on non-droppable should be E0035, got: {:?}", errs);
}

#[test]
fn test_drop_builtin_on_complex_expr_is_e0035() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![Stmt::SExpr {
                loc: loc(),
                expr: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "drop".to_string(),
                    type_args: vec![],
                    args: vec![file_lit()],
                }),
            }],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0035"), "drop on non-variable should be E0035, got: {:?}", errs);
}

#[test]
fn test_drop_in_one_branch_move_in_other_is_consistent() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "consume",
        vec![Param::POwn { name: "f".to_string(), typ: file_typ() }],
        Expr::EVoid { loc: loc() },
        Safety::Safe,
    ));
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SExpr {
                    loc: loc(),
                    expr: Box::new(Expr::EIf {
                        loc: loc(),
                        cond: Box::new(Expr::EBool { loc: loc(), value: true }),
                        then: Box::new(Expr::EBlock {
                            loc: loc(),
                            stmts: vec![drop_call("a")],
                            result: None,
                        }),
                        else_: Some(Box::new(Expr::EBlock {
                            loc: loc(),
                            stmts: vec![Stmt::SExpr {
                                loc: loc(),
                                expr: Box::new(Expr::ECall {
                                    loc: loc(),
                                    name: "consume".to_string(),
                                    type_args: vec![],
                                    args: vec![Expr::EMove { loc: loc(), name: "a".to_string() }],
                                }),
                            }],
                            result: None,
                        })),
                    }),
                },
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(!errs.iter().any(|e| e.code == "E0033"), "drop in one branch + move in other should be consistent, got: {:?}", errs);
}

// ── infectious droppability (recursive glue) ────────────────────

fn handle_typ() -> Typ {
    Typ::TStruct {
        name: "Handle".to_string(),
        fields: vec![],
        type_args: vec![],
    }
}

fn glue_defs() -> Vec<Def> {
    // Handle contains a File but registers no op_drop of its own.
    let mut defs = drop_defs();
    defs.push(make_struct(
        "Handle",
        vec![FieldDef { name: "f".to_string(), typ: file_typ() }],
    ));
    defs
}

#[test]
fn test_glue_struct_is_move_only() {
    let mut defs = glue_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "h".to_string(),
                    typ: handle_typ(),
                    expr: Box::new(Expr::EStructLit {
                        loc: loc(),
                        name: "Handle".to_string(),
                        fields: vec![ValueField {
                            name: "f".to_string(),
                            value: Expr::EMove { loc: loc(), name: "a".to_string() },
                        }],
                        type_args: vec![],
                    }),
                },
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "h2".to_string(),
                    typ: handle_typ(),
                    expr: Box::new(Expr::EVar { loc: loc(), name: "h".to_string() }),
                },
                use_var("h"),
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0001"), "glue struct must be move-only, got: {:?}", errs);
}

#[test]
fn test_deeply_nested_glue_struct_is_move_only() {
    let mut defs = glue_defs();
    defs.push(make_struct(
        "Outer",
        vec![FieldDef {
            name: "h".to_string(),
            typ: handle_typ(),
        }],
    ));
    let outer_typ = Typ::TStruct {
        name: "Outer".to_string(),
        fields: vec![],
        type_args: vec![],
    };
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "h".to_string(),
                    typ: handle_typ(),
                    expr: Box::new(Expr::EStructLit {
                        loc: loc(),
                        name: "Handle".to_string(),
                        fields: vec![ValueField {
                            name: "f".to_string(),
                            value: Expr::EMove { loc: loc(), name: "a".to_string() },
                        }],
                        type_args: vec![],
                    }),
                },
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "o".to_string(),
                    typ: outer_typ.clone(),
                    expr: Box::new(Expr::EStructLit {
                        loc: loc(),
                        name: "Outer".to_string(),
                        fields: vec![ValueField {
                            name: "h".to_string(),
                            value: Expr::EMove { loc: loc(), name: "h".to_string() },
                        }],
                        type_args: vec![],
                    }),
                },
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "o2".to_string(),
                    typ: outer_typ,
                    expr: Box::new(Expr::EVar { loc: loc(), name: "o".to_string() }),
                },
                use_var("o"),
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0001"), "nested glue struct must be move-only, got: {:?}", errs);
}

// ── enum/array infection + generic-argument ban ─────────────────

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

fn slot_typ() -> Typ {
    Typ::TStruct { name: "Slot".to_string(), fields: vec![], type_args: vec![] }
}

#[test]
fn test_enum_with_droppable_payload_is_move_only() {
    let mut defs = drop_defs();
    defs.push(slot_enum());
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "s".to_string(),
                    typ: slot_typ(),
                    expr: Box::new(Expr::ECall {
                        loc: loc(),
                        name: "Slot.Full".to_string(),
                        type_args: vec![],
                        args: vec![Expr::EMove { loc: loc(), name: "a".to_string() }],
                    }),
                },
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "s2".to_string(),
                    typ: slot_typ(),
                    expr: Box::new(Expr::EVar { loc: loc(), name: "s".to_string() }),
                },
                use_var("s"),
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0001"), "enum with droppable payload must be move-only, got: {:?}", errs);
}

#[test]
fn test_array_of_droppable_is_move_only() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![
                let_file("a"),
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "arr".to_string(),
                    typ: Typ::TArray { of: Box::new(file_typ()) },
                    expr: Box::new(Expr::EArrayLit {
                        loc: loc(),
                        values: vec![Expr::EMove { loc: loc(), name: "a".to_string() }],
                    }),
                },
                Stmt::SLetTyped {
                    loc: loc(),
                    name: "arr2".to_string(),
                    typ: Typ::TArray { of: Box::new(file_typ()) },
                    expr: Box::new(Expr::EVar { loc: loc(), name: "arr".to_string() }),
                },
                use_var("arr"),
            ],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0001"), "array of droppable must be move-only, got: {:?}", errs);
}

#[test]
fn test_future_of_droppable_return_is_e0036() {
    let mut defs = drop_defs();
    defs.push(Def::DFunc {
        loc: loc(),
        name: "task".to_string(),
        type_params: vec![],
        params: vec![],
        returns: Some(Typ::TFuture { of: Box::new(file_typ()) }),
        body: Box::new(Expr::EVoid { loc: loc() }),
        safety: Safety::Safe,
        is_async: true,
        type_bounds: vec![],
    });
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0036"), "future[File] must be rejected, got: {:?}", errs);
}

#[test]
fn test_future_of_non_droppable_return_is_ok() {
    let mut defs = drop_defs();
    defs.push(Def::DFunc {
        loc: loc(),
        name: "task".to_string(),
        type_params: vec![],
        params: vec![],
        returns: Some(Typ::TFuture { of: Box::new(Typ::TInt) }),
        body: Box::new(Expr::EVoid { loc: loc() }),
        safety: Safety::Safe,
        is_async: true,
        type_bounds: vec![],
    });
    let errs = check_program(&defs);
    assert!(!errs.iter().any(|e| e.code == "E0036"), "future[int] must be fine, got: {:?}", errs);
}

#[test]
fn test_droppable_in_generic_struct_type_args_is_e0036() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![Stmt::SLetTyped {
                loc: loc(),
                name: "v".to_string(),
                typ: Typ::TStruct {
                    name: "Vec".to_string(),
                    fields: vec![],
                    type_args: vec![file_typ()],
                },
                expr: Box::new(Expr::EVoid { loc: loc() }),
            }],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0036"), "Vec[File] must be rejected, got: {:?}", errs);
}

#[test]
fn test_droppable_in_call_type_args_is_e0036() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![Stmt::SExpr {
                loc: loc(),
                expr: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "identity".to_string(),
                    type_args: vec![file_typ()],
                    args: vec![],
                }),
            }],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0036"), "identity[File]() must be rejected, got: {:?}", errs);
}

#[test]
fn test_droppable_in_box_is_e0036() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![Stmt::SLetTyped {
                loc: loc(),
                name: "b".to_string(),
                typ: Typ::TBox { of: Box::new(file_typ()) },
                expr: Box::new(Expr::EVoid { loc: loc() }),
            }],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(errs.iter().any(|e| e.code == "E0036"), "box<File> must be rejected, got: {:?}", errs);
}

#[test]
fn test_droppable_in_lambda_annotation_is_e0036() {
    let mut defs = drop_defs();
    defs.push(make_func(
        "main",
        vec![],
        Expr::EBlock {
            loc: loc(),
            stmts: vec![Stmt::SLet {
                loc: loc(),
                mutable: false,
                name: "f".to_string(),
                expr: Box::new(Expr::ELambda {
                    loc: loc(),
                    params: vec![Param::POwn {
                        name: "b".to_string(),
                        typ: Typ::TBox { of: Box::new(file_typ()) },
                    }],
                    ret: Typ::TNull,
                    captures: vec![],
                    body: Box::new(Expr::EVoid { loc: loc() }),
                }),
            }],
            result: None,
        },
        Safety::Safe,
    ));
    let errs = check_program(&defs);
    assert!(
        errs.iter().any(|e| e.code == "E0036"),
        "lambda param annotated box<File> must be rejected, got: {:?}",
        errs
    );
}
