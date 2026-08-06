use super::*;

#[test]
fn test_block_let_inference() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EBlock {
                loc: loc(),
                stmts: vec![
                    Stmt::SLet {
                        loc: loc(),
                        mutable: false,
                        name: "x".to_string(),
                        expr: Box::new(Expr::EInt {
                            loc: loc(),
                            value: 42,
                        }),
                    },
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EBinOp {
                            loc: loc(),
                            op: BinOp::Add,
                            left: Box::new(Expr::EVar {
                                loc: loc(),
                                name: "x".to_string(),
                            }),
                            right: Box::new(Expr::EInt {
                                loc: loc(),
                                value: 1,
                            }),
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
        "let inference should work, got: {:?}",
        errs
    );
}

#[test]
fn test_deref_non_pointer() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EDeref {
                loc: loc(),
                expr: Box::new(Expr::EInt {
                    loc: loc(),
                    value: 42,
                }),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "deref non-pointer should error");
    assert!(errs.iter().any(|e| e.code == "E0014"));
}

#[test]
fn test_eq_operator_on_same_types() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EBinOp {
                loc: loc(),
                op: BinOp::Eq,
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
    assert!(errs.is_empty(), "eq on ints should be ok, got: {:?}", errs);
}

#[test]
fn test_eq_operator_type_mismatch() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EBinOp {
                loc: loc(),
                op: BinOp::Eq,
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
    assert!(!errs.is_empty(), "eq on different types should error");
    assert!(errs.iter().any(|e| e.code == "E0014"));
}

#[test]
fn test_nested_blocks() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
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
                    Stmt::SExpr {
                        loc: loc(),
                        expr: Box::new(Expr::EBlock {
                            loc: loc(),
                            stmts: vec![Stmt::SLet {
                                loc: loc(),
                                mutable: false,
                                name: "y".to_string(),
                                expr: Box::new(Expr::EBool {
                                    loc: loc(),
                                    value: true,
                                }),
                            }],
                            result: Some(Box::new(Expr::EVar {
                                loc: loc(),
                                name: "y".to_string(),
                            })),
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EBinOp {
                    loc: loc(),
                    op: BinOp::Add,
                    left: Box::new(Expr::EVar {
                        loc: loc(),
                        name: "x".to_string(),
                    }),
                    right: Box::new(Expr::EInt {
                        loc: loc(),
                        value: 2,
                    }),
                })),
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "nested blocks with type inference should work, got: {:?}",
        errs
    );
}

#[test]
fn test_return_stmt_in_block() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Some(Typ::TInt),
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SReturn {
                    loc: loc(),
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
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "return int from int function should be ok, got: {:?}",
        errs
    );
}

#[test]
fn test_return_stmt_type_mismatch() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            Some(Typ::TInt),
            Expr::EBlock {
                loc: loc(),
                stmts: vec![Stmt::SReturn {
                    loc: loc(),
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
    assert!(
        !errs.is_empty(),
        "return bool from int function should error"
    );
    assert!(errs.iter().any(|e| e.code == "E0017"));
}

#[test]
fn test_generic_identity_call() {
    let defs = vec![
        make_module("test"),
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
            Expr::ECall {
                loc: loc(),
                name: "identity".to_string(),
                type_args: vec![Typ::TInt],
                args: vec![Expr::EInt {
                    loc: loc(),
                    value: 42,
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "expected no type errors, got: {:?}", errs);
}

#[test]
fn test_generic_inference_no_type_args() {
    let defs = vec![
        make_module("test"),
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
            Expr::ECall {
                loc: loc(),
                name: "identity".to_string(),
                type_args: vec![],
                args: vec![Expr::EInt {
                    loc: loc(),
                    value: 42,
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "expected no type errors, got: {:?}", errs);
}

#[test]
fn test_generic_type_arg_mismatch() {
    let defs = vec![
        make_module("test"),
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
            Expr::ECall {
                loc: loc(),
                name: "identity".to_string(),
                type_args: vec![Typ::TInt],
                args: vec![Expr::EString {
                    loc: loc(),
                    value: "hello".to_string(),
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "expected type mismatch error");
}

#[test]
fn test_enum_typecheck_construct_and_match() {
    use crate::ast::*;
    let defs = vec![
        Def::DEnum {
            loc: Loc { line: 1, col: 1 },
            name: "Shape".into(),
            variants: vec![
                EnumVariant {
                    name: "Circle".into(),
                    payload: vec![Typ::TInt],
                },
                EnumVariant {
                    name: "Rect".into(),
                    payload: vec![Typ::TInt, Typ::TInt],
                },
            ],
            type_params: vec![],
        },
        Def::DFunc {
            loc: Loc { line: 2, col: 1 },
            name: "area".into(),
            type_params: vec![],
            params: vec![Param::POwn {
                name: "s".into(),
                typ: Typ::TStruct {
                    name: "Shape".into(),
                    fields: vec![],
                    type_args: vec![],
                },
            }],
            returns: Some(Typ::TInt),
            body: Box::new(Expr::EChoose {
                loc: Loc { line: 3, col: 1 },
                var: Box::new(Expr::EVar {
                    loc: Loc { line: 3, col: 1 },
                    name: "s".into(),
                }),
                cases: vec![WhenCase {
                    when: Box::new(Expr::EFieldAccess {
                        loc: Loc { line: 4, col: 1 },
                        expr: Box::new(Expr::EVar {
                            loc: Loc { line: 4, col: 1 },
                            name: "Shape".into(),
                        }),
                        field: "Circle".into(),
                    }),
                    guard: None,
                    then: Box::new(Expr::EInt {
                        loc: Loc { line: 4, col: 1 },
                        value: 1,
                    }),
                }],
                otherwise: Some(Box::new(Expr::EInt {
                    loc: Loc { line: 5, col: 1 },
                    value: 0,
                })),
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        },
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
}

#[test]
fn test_enum_pattern_destructure_typecheck() {
    use crate::ast::*;
    let defs = vec![
        Def::DEnum {
            loc: Loc { line: 1, col: 1 },
            name: "Shape".into(),
            variants: vec![
                EnumVariant {
                    name: "Circle".into(),
                    payload: vec![Typ::TInt],
                },
                EnumVariant {
                    name: "Rect".into(),
                    payload: vec![Typ::TInt, Typ::TInt],
                },
            ],
            type_params: vec![],
        },
        Def::DFunc {
            loc: Loc { line: 2, col: 1 },
            name: "area".into(),
            type_params: vec![],
            params: vec![Param::POwn {
                name: "s".into(),
                typ: Typ::TStruct {
                    name: "Shape".into(),
                    fields: vec![],
                    type_args: vec![],
                },
            }],
            returns: Some(Typ::TInt),
            body: Box::new(Expr::EChoose {
                loc: Loc { line: 3, col: 1 },
                var: Box::new(Expr::EVar {
                    loc: Loc { line: 3, col: 1 },
                    name: "s".into(),
                }),
                cases: vec![
                    WhenCase {
                        when: Box::new(Expr::EEnumPattern {
                            loc: Loc { line: 4, col: 1 },
                            enum_name: "Shape".into(),
                            variant: "Circle".into(),
                            bindings: vec!["r".into()],
                        }),
                        guard: None,
                        then: Box::new(Expr::EBinOp {
                            loc: Loc { line: 4, col: 1 },
                            op: BinOp::Mul,
                            left: Box::new(Expr::EVar {
                                loc: Loc { line: 4, col: 1 },
                                name: "r".into(),
                            }),
                            right: Box::new(Expr::EVar {
                                loc: Loc { line: 4, col: 1 },
                                name: "r".into(),
                            }),
                        }),
                    },
                    WhenCase {
                        when: Box::new(Expr::EEnumPattern {
                            loc: Loc { line: 5, col: 1 },
                            enum_name: "Shape".into(),
                            variant: "Rect".into(),
                            bindings: vec!["w".into(), "h".into()],
                        }),
                        guard: None,
                        then: Box::new(Expr::EBinOp {
                            loc: Loc { line: 5, col: 1 },
                            op: BinOp::Add,
                            left: Box::new(Expr::EVar {
                                loc: Loc { line: 5, col: 1 },
                                name: "w".into(),
                            }),
                            right: Box::new(Expr::EVar {
                                loc: Loc { line: 5, col: 1 },
                                name: "h".into(),
                            }),
                        }),
                    },
                ],
                otherwise: Some(Box::new(Expr::EInt {
                    loc: Loc { line: 6, col: 1 },
                    value: 0,
                })),
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        },
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
}

#[test]
fn test_enum_pattern_binding_arity_mismatch() {
    use crate::ast::*;
    let defs = vec![
        Def::DEnum {
            loc: Loc { line: 1, col: 1 },
            name: "Shape".into(),
            variants: vec![EnumVariant {
                name: "Circle".into(),
                payload: vec![Typ::TInt],
            }],
            type_params: vec![],
        },
        Def::DFunc {
            loc: Loc { line: 2, col: 1 },
            name: "area".into(),
            type_params: vec![],
            params: vec![Param::POwn {
                name: "s".into(),
                typ: Typ::TStruct {
                    name: "Shape".into(),
                    fields: vec![],
                    type_args: vec![],
                },
            }],
            returns: Some(Typ::TInt),
            body: Box::new(Expr::EChoose {
                loc: Loc { line: 3, col: 1 },
                var: Box::new(Expr::EVar {
                    loc: Loc { line: 3, col: 1 },
                    name: "s".into(),
                }),
                cases: vec![WhenCase {
                    when: Box::new(Expr::EEnumPattern {
                        loc: Loc { line: 4, col: 1 },
                        enum_name: "Shape".into(),
                        variant: "Circle".into(),
                        bindings: vec!["a".into(), "b".into()],
                    }),
                    guard: None,
                    then: Box::new(Expr::EInt {
                        loc: Loc { line: 4, col: 1 },
                        value: 1,
                    }),
                }],
                otherwise: Some(Box::new(Expr::EInt {
                    loc: Loc { line: 5, col: 1 },
                    value: 0,
                })),
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        },
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty());
    assert!(errs.iter().any(|e| e.code == "E0016"));
}

#[test]
fn test_generic_enum_typecheck_construct_and_match() {
    use crate::ast::*;
    let defs = vec![
        Def::DEnum {
            loc: Loc { line: 1, col: 1 },
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
        },
        Def::DFunc {
            loc: Loc { line: 2, col: 1 },
            name: "main".into(),
            type_params: vec![],
            params: vec![],
            returns: Some(Typ::TInt),
            body: Box::new(Expr::EBlock {
                loc: Loc { line: 3, col: 1 },
                stmts: vec![
                    Stmt::SLetTyped {
                        loc: Loc { line: 4, col: 1 },
                        name: "b".into(),
                        typ: Typ::TStruct {
                            name: "Box".into(),
                            fields: vec![],
                            type_args: vec![Typ::TInt],
                        },
                        expr: Box::new(Expr::ECall {
                            loc: Loc { line: 4, col: 1 },
                            name: "Value".into(),
                            type_args: vec![Typ::TInt],
                            args: vec![
                                Expr::EVar {
                                    loc: Loc { line: 4, col: 1 },
                                    name: "Box".into(),
                                },
                                Expr::EInt {
                                    loc: Loc { line: 4, col: 1 },
                                    value: 1,
                                },
                            ],
                        }),
                    },
                    Stmt::SExpr {
                        loc: Loc { line: 5, col: 1 },
                        expr: Box::new(Expr::EChoose {
                            loc: Loc { line: 5, col: 1 },
                            var: Box::new(Expr::EVar {
                                loc: Loc { line: 5, col: 1 },
                                name: "b".into(),
                            }),
                            cases: vec![WhenCase {
                                when: Box::new(Expr::EEnumPattern {
                                    loc: Loc { line: 6, col: 1 },
                                    enum_name: "Box".into(),
                                    variant: "Value".into(),
                                    bindings: vec!["v".into()],
                                }),
                                guard: None,
                                then: Box::new(Expr::EVar {
                                    loc: Loc { line: 6, col: 1 },
                                    name: "v".into(),
                                }),
                            }],
                            otherwise: Some(Box::new(Expr::EInt {
                                loc: Loc { line: 7, col: 1 },
                                value: 0,
                            })),
                        }),
                    },
                    Stmt::SReturn {
                        loc: Loc { line: 8, col: 1 },
                        expr: Box::new(Expr::EInt {
                            loc: Loc { line: 8, col: 1 },
                            value: 1,
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EInt {
                    loc: Loc { line: 9, col: 1 },
                    value: 0,
                })),
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        },
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
}

#[test]
fn test_generic_enum_type_mismatch() {
    use crate::ast::*;
    let defs = vec![
        Def::DEnum {
            loc: Loc { line: 1, col: 1 },
            name: "Box".into(),
            variants: vec![EnumVariant {
                name: "Value".into(),
                payload: vec![Typ::TGenericParam { name: "T".into() }],
            }],
            type_params: vec!["T".into()],
        },
        Def::DFunc {
            loc: Loc { line: 2, col: 1 },
            name: "f".into(),
            type_params: vec![],
            params: vec![Param::POwn {
                name: "b".into(),
                typ: Typ::TStruct {
                    name: "Box".into(),
                    fields: vec![],
                    type_args: vec![Typ::TInt],
                },
            }],
            returns: Some(Typ::TInt),
            body: Box::new(Expr::EInt {
                loc: Loc { line: 3, col: 1 },
                value: 0,
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        },
        Def::DFunc {
            loc: Loc { line: 4, col: 1 },
            name: "main".into(),
            type_params: vec![],
            params: vec![],
            returns: Some(Typ::TInt),
            body: Box::new(Expr::EBlock {
                loc: Loc { line: 5, col: 1 },
                stmts: vec![
                    Stmt::SLetTyped {
                        loc: Loc { line: 6, col: 1 },
                        name: "b".into(),
                        typ: Typ::TStruct {
                            name: "Box".into(),
                            fields: vec![],
                            type_args: vec![Typ::TString],
                        },
                        expr: Box::new(Expr::ECall {
                            loc: Loc { line: 6, col: 1 },
                            name: "Value".into(),
                            type_args: vec![Typ::TString],
                            args: vec![
                                Expr::EVar {
                                    loc: Loc { line: 6, col: 1 },
                                    name: "Box".into(),
                                },
                                Expr::EString {
                                    loc: Loc { line: 6, col: 1 },
                                    value: "x".into(),
                                },
                            ],
                        }),
                    },
                    Stmt::SExpr {
                        loc: Loc { line: 7, col: 1 },
                        expr: Box::new(Expr::ECall {
                            loc: Loc { line: 7, col: 1 },
                            name: "f".into(),
                            type_args: vec![],
                            args: vec![Expr::EVar {
                                loc: Loc { line: 7, col: 1 },
                                name: "b".into(),
                            }],
                        }),
                    },
                ],
                result: Some(Box::new(Expr::EInt {
                    loc: Loc { line: 8, col: 1 },
                    value: 0,
                })),
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        },
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "expected a type error but got none");
    assert!(errs.iter().any(|e| e.code == "E0016"));
}

#[test]
fn test_func_return_type_mismatch() {
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
        make_func(
            "bad_return",
            vec![],
            Some(Typ::TInt),
            Expr::EFloat {
                loc: loc(),
                value: 3.14,
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        !errs.is_empty(),
        "returning float from int function should error"
    );
    assert!(errs.iter().any(|e| e.code == "E0017"));
}

#[test]
fn test_func_return_type_void_ok() {
    let defs = vec![
        make_module("test"),
        make_func(
            "main",
            vec![],
            None,
            Expr::EVoid { loc: loc() },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(errs.is_empty(), "void return should be ok, got: {:?}", errs);
}

#[test]
fn test_struct_literal_field_type_mismatch() {
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
                        value: Expr::EBool {
                            loc: loc(),
                            value: true,
                        },
                    },
                ],
                type_args: vec![],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "struct field type mismatch should error");
    assert!(errs.iter().any(|e| e.code == "E0018"));
}

#[test]
fn test_func_arg_type_mismatch() {
    let defs = vec![
        make_module("test"),
        make_func(
            "add",
            vec![Param::POwn {
                name: "a".to_string(),
                typ: Typ::TInt,
            }],
            Some(Typ::TInt),
            Expr::EVar {
                loc: loc(),
                name: "a".to_string(),
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
                args: vec![Expr::EBool {
                    loc: loc(),
                    value: true,
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(!errs.is_empty(), "passing bool to int param should error");
    assert!(errs.iter().any(|e| e.code == "E0016"));
}

#[test]
fn test_valid_func_call_with_correct_types() {
    let defs = vec![
        make_module("test"),
        make_func(
            "add",
            vec![Param::POwn {
                name: "a".to_string(),
                typ: Typ::TInt,
            }],
            Some(Typ::TInt),
            Expr::EVar {
                loc: loc(),
                name: "a".to_string(),
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
                args: vec![Expr::EInt {
                    loc: loc(),
                    value: 5,
                }],
            },
            Safety::Safe,
        ),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "valid call should have no errors, got: {:?}",
        errs
    );
}

fn file_struct_typ() -> Typ {
    Typ::TStruct {
        name: "File".to_string(),
        fields: vec![],
        type_args: vec![],
    }
}

fn make_drop_impl(struct_name: &str, func: &str) -> Def {
    Def::DImpl {
        loc: loc(),
        struct_name: struct_name.to_string(),
        impls: vec![ImplExpr {
            op: ImplOp::ImDrop,
            func: func.to_string(),
            loc: loc(),
        }],
    }
}

fn make_file_close(name: &str, params: Vec<Param>, returns: Option<Typ>) -> Def {
    make_func(
        name,
        params,
        returns,
        Expr::EVoid { loc: loc() },
        Safety::Safe,
    )
}

#[test]
fn test_op_drop_valid_registration() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "File",
            vec![FieldDef {
                name: "fd".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_file_close(
            "file_close",
            vec![Param::PRef {
                name: "self".to_string(),
                typ: file_struct_typ(),
            }],
            None,
        ),
        make_drop_impl("File", "file_close"),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "valid op_drop should have no errors, got: {:?}",
        errs
    );
}

#[test]
fn test_op_drop_rejects_own_param() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "File",
            vec![FieldDef {
                name: "fd".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_file_close(
            "file_close",
            vec![Param::POwn {
                name: "self".to_string(),
                typ: file_struct_typ(),
            }],
            None,
        ),
        make_drop_impl("File", "file_close"),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.iter().any(|e| e.code == "E0031"),
        "own param should be E0031, got: {:?}",
        errs
    );
}

#[test]
fn test_op_drop_rejects_wrong_param_type() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "File",
            vec![FieldDef {
                name: "fd".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_file_close(
            "file_close",
            vec![Param::PRef {
                name: "self".to_string(),
                typ: Typ::TInt,
            }],
            None,
        ),
        make_drop_impl("File", "file_close"),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.iter().any(|e| e.code == "E0031"),
        "wrong param type should be E0031, got: {:?}",
        errs
    );
}

#[test]
fn test_op_drop_rejects_return_value() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "File",
            vec![FieldDef {
                name: "fd".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_func(
            "file_close",
            vec![Param::PRef {
                name: "self".to_string(),
                typ: file_struct_typ(),
            }],
            Some(Typ::TInt),
            Expr::EInt {
                loc: loc(),
                value: 0,
            },
            Safety::Safe,
        ),
        make_drop_impl("File", "file_close"),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.iter().any(|e| e.code == "E0031"),
        "return value should be E0031, got: {:?}",
        errs
    );
}

#[test]
fn test_op_drop_rejects_unknown_function() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "File",
            vec![FieldDef {
                name: "fd".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_drop_impl("File", "no_such_fn"),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.iter().any(|e| e.code == "E0031"),
        "unknown function should be E0031, got: {:?}",
        errs
    );
}

#[test]
fn test_op_drop_rejects_duplicate_registration() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "File",
            vec![FieldDef {
                name: "fd".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_file_close(
            "file_close",
            vec![Param::PRef {
                name: "self".to_string(),
                typ: file_struct_typ(),
            }],
            None,
        ),
        make_file_close(
            "file_close2",
            vec![Param::PRef {
                name: "self".to_string(),
                typ: file_struct_typ(),
            }],
            None,
        ),
        make_drop_impl("File", "file_close"),
        make_drop_impl("File", "file_close2"),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.iter().any(|e| e.code == "E0032"),
        "duplicate op_drop should be E0032, got: {:?}",
        errs
    );
}

fn sealed_defs(main_body: Expr) -> Vec<Def> {
    vec![
        make_module("test"),
        make_struct(
            "File",
            vec![FieldDef {
                name: "fd".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_file_close(
            "file_close",
            vec![Param::PRef {
                name: "self".to_string(),
                typ: file_struct_typ(),
            }],
            None,
        ),
        make_drop_impl("File", "file_close"),
        make_func("main", vec![], None, main_body, Safety::Safe),
    ]
}

#[test]
fn test_sealed_drop_fn_direct_call_is_e0034() {
    let defs = sealed_defs(Expr::EBlock {
        loc: loc(),
        stmts: vec![
            Stmt::SLetTyped {
                loc: loc(),
                name: "f".to_string(),
                typ: file_struct_typ(),
                expr: Box::new(Expr::EStructLit {
                    loc: loc(),
                    name: "File".to_string(),
                    type_args: vec![],
                    fields: vec![ValueField {
                        name: "fd".to_string(),
                        value: Expr::EInt {
                            loc: loc(),
                            value: 1,
                        },
                    }],
                }),
            },
            Stmt::SExpr {
                loc: loc(),
                expr: Box::new(Expr::ECall {
                    loc: loc(),
                    name: "file_close".to_string(),
                    type_args: vec![],
                    args: vec![Expr::EVar {
                        loc: loc(),
                        name: "f".to_string(),
                    }],
                }),
            },
        ],
        result: None,
    });
    let errs = check_program(&defs);
    assert!(
        errs.iter().any(|e| e.code == "E0034"),
        "direct call of sealed drop fn should be E0034, got: {:?}",
        errs
    );
}

#[test]
fn test_sealed_drop_fn_value_use_is_e0034() {
    let defs = sealed_defs(Expr::EBlock {
        loc: loc(),
        stmts: vec![Stmt::SLet {
            loc: loc(),
            mutable: false,
            name: "g".to_string(),
            expr: Box::new(Expr::EVar {
                loc: loc(),
                name: "file_close".to_string(),
            }),
        }],
        result: None,
    });
    let errs = check_program(&defs);
    assert!(
        errs.iter().any(|e| e.code == "E0034"),
        "value use of sealed drop fn should be E0034, got: {:?}",
        errs
    );
}

#[test]
fn test_drop_fn_body_may_reference_self() {
    let defs = vec![
        make_module("test"),
        make_struct(
            "File",
            vec![FieldDef {
                name: "fd".to_string(),
                typ: Typ::TInt,
            }],
        ),
        make_file_close(
            "file_close",
            vec![Param::PRef {
                name: "self".to_string(),
                typ: file_struct_typ(),
            }],
            None,
        ),
        make_drop_impl("File", "file_close"),
    ];
    let errs = check_program(&defs);
    assert!(
        errs.is_empty(),
        "drop fn definition itself should not trigger E0034, got: {:?}",
        errs
    );
}
