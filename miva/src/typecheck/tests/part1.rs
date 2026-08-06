use super::*;

#[test]
fn test_empty_program_no_type_errors() {
    let defs = vec![make_module("test")];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "empty program should have no type errors");
}

#[test]
fn test_inferred_droppable_generic_arg_is_e0036() {
    let file_typ = Typ::TStruct {
        name: "File".to_string(),
        fields: vec![],
        type_args: vec![],
    };
    let defs = vec![
        make_module("test"),
        make_struct(
            "File",
            vec![FieldDef {
                name: "id".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_func(
            "file_close",
            vec![Param::PRef {
                name: "self".to_string(),
                typ: file_typ.clone(),
            }],
            None,
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ),
        Def::DImpl {
            loc: loc(),
            struct_name: "File".to_string(),
            impls: vec![ImplExpr {
                op: ImplOp::ImDrop,
                func: "file_close".to_string(),
                loc: loc(),
            }],
        },
        Def::DFunc {
            loc: loc(),
            name: "identity".to_string(),
            type_params: vec!["T".to_string()],
            params: vec![Param::POwn {
                name: "x".to_string(),
                typ: Typ::TGenericParam {
                    name: "T".to_string(),
                },
            }],
            returns: Some(Typ::TGenericParam {
                name: "T".to_string(),
            }),
            body: Box::new(Expr::EVar {
                loc: loc(),
                name: "x".to_string(),
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        },
        make_func(
            "main",
            vec![],
            None,
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SLetTyped {
                        loc: loc(),
                        name: "f".to_string(),
                        typ: file_typ.clone(),
                        expr: Box::new(Expr::EStructLit {
                            loc: loc(),
                            name: "File".to_string(),
                            fields: vec![ValueField {
                                name: "id".to_string(),
                                value: Expr::EInt {
                                    loc: loc(),
                                    value: 1,
                                },
                            }],
                            type_args: vec![],
                        }),
                    },
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::ECall {
                            loc: loc(),
                            name: "identity".to_string(),
                            type_args: vec![],
                            args: vec![Expr::EMove {
                                loc: loc(),
                                name: "f".to_string(),
                            }],
                        }),
                    },
                ],
                result: None,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.iter().any(|e| e.code == "E0036"),
        "inferred T=File for generic call must be E0036, got: {:?}",
        errs
    );
}

#[test]
fn test_valid_int_addition() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EBinOp {
                loc: loc(),
                op: BinOp::Add,
                left: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 1,
                }),
                right: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 2,
                }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "int + int should be valid, got: {:?}",
        errs
    );
}

#[test]
fn test_type_mismatch_binop() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EBinOp {
                loc: loc(),
                op: BinOp::Add,
                left: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 1,
                }),
                right: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "int + bool should be a type error");
    assert!(errs.iter().any(|e| e.code == "E0014"));
}

#[test]
fn test_if_condition_must_be_bool() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EIf {
                loc: loc(),
                cond: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 0,
                }),
                then: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 1,
                }),
                else_: Some(Box::new(Expr::EInt {
                    loc: loc(),
                    value: 2,
                })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "if condition with int should error");
    assert!(errs.iter().any(|e| e.code == "E0014"));
}

#[test]
fn test_if_else_type_mismatch() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EIf {
                loc: loc(),
                cond: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                then: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 1,
                }),
                else_: Some(Box::new(Expr::EBool {
                    loc: loc(),
                    value: false,
                })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "if/else type mismatch should error");
    assert!(errs.iter().any(|e| e.code == "E0014"));
}

#[test]
fn test_if_else_same_type_ok() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EIf {
                loc: loc(),
                cond: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                then: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 1,
                }),
                else_: Some(Box::new(Expr::EInt {
                    loc: loc(),
                    value: 2,
                })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "if/else same type should be ok, got: {:?}",
        errs
    );
}

#[test]
fn test_if_void_branch_no_else_ok() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EIf {
                loc: loc(),
                cond: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                then: Box::new(Expr::EVoid { loc: loc() }),
                else_: None,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "if with void then and no else should be ok, got: {:?}",
        errs
    );
}

#[test]
fn test_if_void_then_void_else_ok() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EIf {
                loc: loc(),
                cond: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                then: Box::new(Expr::EVoid { loc: loc() }),
                else_: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "if with both void branches should be ok, got: {:?}",
        errs
    );
}

#[test]
fn test_fn_call_arg_type_mismatch() {
    let defs = vec![
        make_module("test"),
        make_func(
            "needs_int",
            vec![Param::POwn {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
            None,
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ),
        make_func(
            "main",
            vec![],
            None,
            Expr::ECall {
                loc: loc(),
                name: "needs_int".to_string(),
                type_args: vec![],
                args: vec![Expr::EBool {
                    loc: loc(),
                    value: true,
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "arg type mismatch should error");
    assert!(errs.iter().any(|e| e.code == "E0016"));
}

#[test]
fn test_fn_call_arg_count_mismatch() {
    let defs = vec![
        make_module("test"),
        make_func(
            "two_args",
            vec![
                Param::POwn {
                    name: "a".to_string(),
                    typ: Typ::TInt,
                },
                Param::POwn {
                    name: "b".to_string(),
                    typ: Typ::TInt,
                },
            ],
            None,
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ),
        make_func(
            "main",
            vec![],
            None,
            Expr::ECall {
                loc: loc(),
                name: "two_args".to_string(),
                type_args: vec![],
                args: vec![Expr::EInt {
                    loc: loc(),
                    value: 1,
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "arg count mismatch should error");
    assert!(errs.iter().any(|e| e.code == "E0016"));
}

#[test]
fn test_fn_call_correct_args() {
    let defs = vec![
        make_module("test"),
        make_func(
            "add",
            vec![
                Param::POwn {
                    name: "a".to_string(),
                    typ: Typ::TInt,
                },
                Param::POwn {
                    name: "b".to_string(),
                    typ: Typ::TInt,
                },
            ],
            Some(Typ::TInt),
            Expr::EBinOp {
                loc: loc(),
                op: BinOp::Add,
                left: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "a".to_string(),
                }),
                right: Box::new(Expr::EVar {
                    loc: loc(),
                    name: "b".to_string(),
                }),
            },
            Safety::Safe,
        ),
        make_func(
            "main",
            vec![],
            None,
            Expr::ECall {
                loc: loc(),
                name: "add".to_string(),
                type_args: vec![],
                args: vec![
                    Expr::EInt {
                        loc: loc(),
                        value: 1,
                    },
                    Expr::EInt {
                        loc: loc(),
                        value: 2,
                    },
                ],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "correct args should be ok, got: {:?}",
        errs
    );
}

#[test]
fn test_struct_literal_type_check() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "Point",
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
        ),
        make_func(
            "main",
            vec![],
            None,
            Expr::EStructLit {
                loc: loc(),
                name: "Point".to_string(),
                type_args: vec![],
                fields: vec![
                    ValueField {
                        name: "x".to_string(),
                        value: Expr::EInt {
                            loc: loc(),
                            value: 1,
                        },
                    },
                    ValueField {
                        name: "y".to_string(),
                        value: Expr::EInt {
                            loc: loc(),
                            value: 2,
                        },
                    },
                ],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "correct struct lit should be ok, got: {:?}",
        errs
    );
}

#[test]
fn test_struct_literal_wrong_field_type() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "Point",
            vec![FieldDef {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_func(
            "main",
            vec![],
            None,
            Expr::EStructLit {
                loc: loc(),
                name: "Point".to_string(),
                type_args: vec![],
                fields: vec![ValueField {
                    name: "x".to_string(),
                    value: Expr::EBool {
                        loc: loc(),
                        value: true,
                    },
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "wrong field type should error");
    assert!(errs.iter().any(|e| e.code == "E0018"));
}

#[test]
fn test_struct_literal_missing_field() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "Point",
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
        ),
        make_func(
            "main",
            vec![],
            None,
            Expr::EStructLit {
                loc: loc(),
                name: "Point".to_string(),
                type_args: vec![],
                fields: vec![ValueField {
                    name: "x".to_string(),
                    value: Expr::EInt {
                        loc: loc(),
                        value: 1,
                    },
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "missing field should error");
    assert!(errs.iter().any(|e| e.code == "E0018"));
}

#[test]
fn test_unknown_struct() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EStructLit {
                loc: loc(),
                name: "NonExistent".to_string(),
                type_args: vec![],
                fields: vec![],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "unknown struct should error");
    assert!(errs.iter().any(|e| e.code == "E0018"));
}

#[test]
fn test_field_access_ok() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "Point",
            vec![FieldDef {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_func(
            "main",
            vec![],
            None,
            Expr::EFieldAccess {
                loc: loc(),
                expr: Box::new(Expr::EStructLit {
                    loc: loc(),
                    name: "Point".to_string(),
                    type_args: vec![],
                    fields: vec![ValueField {
                        name: "x".to_string(),
                        value: Expr::EInt {
                            loc: loc(),
                            value: 1,
                        },
                    }],
                }),
                field: "x".to_string(),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "valid field access should be ok, got: {:?}",
        errs
    );
}

#[test]
fn test_field_access_unknown_field() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "Point",
            vec![FieldDef {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_func(
            "main",
            vec![],
            None,
            Expr::EFieldAccess {
                loc: loc(),
                expr: Box::new(Expr::EStructLit {
                    loc: loc(),
                    name: "Point".to_string(),
                    type_args: vec![],
                    fields: vec![ValueField {
                        name: "x".to_string(),
                        value: Expr::EInt {
                            loc: loc(),
                            value: 1,
                        },
                    }],
                }),
                field: "z".to_string(),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "unknown field should error");
    assert!(errs.iter().any(|e| e.code == "E0019"));
}

#[test]
fn test_return_type_mismatch() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Some(Typ::TInt),
            Expr::EBool {
                loc: loc(),
                value: true,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "return type mismatch should error");
    assert!(errs.iter().any(|e| e.code == "E0017"));
}

#[test]
fn test_return_type_match_ok() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Some(Typ::TInt),
            Expr::EInt {
                loc: loc(),
                value: 42,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "correct return type should be ok, got: {:?}",
        errs
    );
}

#[test]
fn test_assignment_type_mismatch() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![Param::POwn {
                name: "x".to_string(),
                typ: Typ::TInt,
            }],
            None,
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SAssign {
                    loc: loc(),
                    name: "x".to_string(),
                    expr: Box::new(Expr::EBool {
                        loc: loc(),
                        value: true,
                    }),
                }],
                result: Some(Box::new(Expr::EVoid { loc: loc() })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "assign bool to int should error");
    assert!(errs.iter().any(|e| e.code == "E0022"));
}

#[test]
fn test_array_type_consistency() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EArrayLit {
                loc: loc(),
                values: vec![
                    Expr::EInt {
                        loc: loc(),
                        value: 1,
                    },
                    Expr::EBool {
                        loc: loc(),
                        value: true,
                    },
                ],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "mixed array types should error");
    assert!(errs.iter().any(|e| e.code == "E0024"));
}

#[test]
fn test_array_type_homogeneous_ok() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EArrayLit {
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
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "homogeneous array should be ok, got: {:?}",
        errs
    );
}

#[test]
fn test_invalid_cast() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::ECast {
                loc: loc(),
                expr: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                to: Typ::TString,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "invalid cast should error");
    assert!(errs.iter().any(|e| e.code == "E0021"));
}

#[test]
fn test_valid_cast() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::ECast {
                loc: loc(),
                expr: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 65,
                }),
                to: Typ::TChar,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "int to char cast should be valid, got: {:?}",
        errs
    );
}

#[test]
fn test_while_condition_must_be_bool() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EWhile {
                loc: loc(),
                cond: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 1,
                }),
                body: Box::new(Expr::EVoid { loc: loc() }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "while condition must be bool");
    assert!(errs.iter().any(|e| e.code == "E0014"));
}

#[test]
fn test_while_valid() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EWhile {
                loc: loc(),
                cond: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                body: Box::new(Expr::EVoid { loc: loc() }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "valid while should be ok, got: {:?}", errs);
}

#[test]
fn test_for_loop_range_type() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EFor {
                loc: loc(),
                var: "i".to_string(),
                range: Box::new(Expr::EBool {
                    loc: loc(),
                    value: true,
                }),
                body: Box::new(Expr::EVoid { loc: loc() }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "for range must be array");
    assert!(errs.iter().any(|e| e.code == "E0026"));
}
