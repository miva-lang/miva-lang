use super::*;

pub fn emit_expr(expr: &IrExpr, depth: usize, expected_type: Option<&str>) -> String {
    match expr {
        IrExpr::Int(value) => format!("static_cast<mvp_builtin_int>({})", value),
        IrExpr::Bool(value) => format!(
            "mvp_builtin_boolean({})",
            if *value { "true" } else { "false" }
        ),
        IrExpr::Float(value) => format!("mvp_builtin_float({})", value.to_string()),
        IrExpr::Char(value) => format!("mvp_builtin_byte('{}')", cxx_escape_string(value)),
        IrExpr::String(value) => {
            format!("mvp_builtin_string(\"{}\")", cxx_escape_string(value))
        }
        IrExpr::Var(name) => mangle_cpp_kw(name),
        IrExpr::Move(name) => format!("std::move({})", mangle_cpp_kw(name)),
        IrExpr::Clone(name) => {
            format!("decltype({})({})", mangle_cpp_kw(name), mangle_cpp_kw(name))
        }
        IrExpr::Void => "mvp_builtin_void".into(),
        IrExpr::Call {
            name,
            type_args,
            args,
        } => emit_call(name, type_args, args, depth, expected_type),
        IrExpr::MethodCall {
            target,
            method,
            type_args,
            args,
        } => {
            unreachable!("EMethodCall should not reach emitter")
        }
        IrExpr::BinOp { op, left, right } => emit_binop(op, left, right, depth, expected_type),
        IrExpr::FieldAccess { expr, field } => emit_field_access(expr, field, depth, expected_type),
        IrExpr::StructInit {
            name,
            type_args,
            fields,
        } => emit_struct_lit(name, type_args, fields, depth, expected_type),
        IrExpr::ArrayInit(values) => {
            let elems: Vec<_> = values
                .iter()
                .map(|e| emit_expr(e, depth, expected_type))
                .collect();
            format!("std::vector{{{}}}", elems.join(", "))
        }
        IrExpr::TupleInit(values) => {
            let elems: Vec<_> = values
                .iter()
                .map(|e| emit_expr(e, depth, expected_type))
                .collect();
            format!("std::make_tuple({})", elems.join(", "))
        }
        IrExpr::Cast { expr, to } => {
            let from_int = matches!(expr.as_ref(), IrExpr::Int { .. })
                || matches!(expr.as_ref(), IrExpr::Var { .. });
            let is_ptr_cast = matches!(to, Typ::TPtrAny) || from_int && matches!(to, Typ::TPtrAny);
            if is_ptr_cast {
                format!(
                    "reinterpret_cast<{}>({})",
                    cxx_type(to),
                    emit_expr(expr, depth, expected_type)
                )
            } else {
                format!(
                    "static_cast<{}>({})",
                    cxx_type(to),
                    emit_expr(expr, depth, expected_type)
                )
            }
        }
        IrExpr::Addr(expr) => format!("&({})", emit_expr(expr, depth, expected_type)),
        IrExpr::Deref(expr) => format!("*({})", emit_expr(expr, depth, expected_type)),
        IrExpr::IfValue {
            cond,
            then,
            else_,
            has_panic,
        } => emit_if(cond, then, else_, *has_panic, depth, expected_type),
        IrExpr::Block { stmts, result } => emit_block(stmts, result, depth, expected_type),
        IrExpr::While { cond, body, result } => {
            emit_while(cond, body, result, depth, expected_type)
        }
        IrExpr::Loop { body, result } => emit_loop(body, result, depth, expected_type),
        IrExpr::For {
            var,
            range,
            body,
            result,
        } => emit_for(var, range, body, result, depth, expected_type),
        IrExpr::ClosureRef { id } => emit_closure_ref(*id),
        IrExpr::Choose {
            var,
            cases,
            otherwise,
            has_panic,
        } => emit_choose(var, cases, otherwise, *has_panic, depth, expected_type),
        IrExpr::Macro { .. } => String::new(),
    }
}

pub(crate) fn emit_call(
    name: &str,
    type_args: &[Typ],
    args: &[IrExpr],
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    let args_strs: Vec<_> = args
        .iter()
        .map(|a| emit_expr(a, depth, expected_type))
        .collect();
    let type_arg_str = if type_args.is_empty() {
        String::new()
    } else {
        let tas: Vec<_> = type_args.iter().map(cxx_type).collect();
        format!("<{}>", tas.join(", "))
    };
    if name.matches('.').count() == 1 {
        let dot = name.find('.').unwrap();
        let enum_name = &name[..dot];
        let variant = &name[dot + 1..];
        return format!(
            "{}_{}{}({})",
            enum_name,
            variant,
            type_arg_str,
            args_strs.join(", ")
        );
    } else if let Some(enum_name) = args.first().and_then(|a| match a {
        IrExpr::Var(n) => Some(n.as_str()),
        _ => None,
    }) {
        if enum_name.starts_with(|c: char| c.is_uppercase()) {
            let payload_strs = &args_strs[1..];
            return format!(
                "{}_{}{}({})",
                enum_name,
                name,
                type_arg_str,
                payload_strs.join(", ")
            );
        }
    }
    format!(
        "{}{}({})",
        map_builtin(name),
        type_arg_str,
        args_strs.join(", ")
    )
}

pub(crate) fn emit_binop(
    op: &BinOp,
    left: &IrExpr,
    right: &IrExpr,
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    let op_str = match op {
        BinOp::Add => " + ",
        BinOp::Sub => " - ",
        BinOp::Mul => " * ",
        BinOp::Div => " / ",
        BinOp::Eq => " == ",
        BinOp::Neq => " != ",
        BinOp::Lt => " < ",
        BinOp::Gt => " > ",
        BinOp::Le => " <= ",
        BinOp::Ge => " >= ",
        BinOp::And => " && ",
        BinOp::Or => " || ",
    };
    format!(
        "({}{}{})",
        emit_expr(left, depth, expected_type),
        op_str,
        emit_expr(right, depth, expected_type)
    )
}

pub(crate) fn emit_field_access(
    expr: &IrExpr,
    field: &str,
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    if field.chars().all(|c| c.is_ascii_digit()) {
        let idx: usize = field.parse().unwrap_or(0);
        format!(
            "std::get<{}>({})",
            idx,
            emit_expr(expr, depth, expected_type)
        )
    } else if let IrExpr::Var(enum_name) = expr {
        if enum_name.chars().next().map_or(false, |c| c.is_uppercase()) {
            format!("{}_{}()", enum_name, field)
        } else {
            format!("{}.{}", emit_expr(expr, depth, expected_type), field)
        }
    } else {
        format!("{}.{}", emit_expr(expr, depth, expected_type), field)
    }
}

pub(crate) fn emit_struct_lit(
    name: &str,
    type_args: &[Typ],
    fields: &[(String, IrExpr)],
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    let type_name = if type_args.is_empty() {
        name.to_string()
    } else {
        let args_str = type_args
            .iter()
            .map(cxx_type)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}<{}>", name, args_str)
    };
    if fields.is_empty() {
        format!("{}{{}}", type_name)
    } else {
        let temp = "__temp";
        let inits: Vec<_> = fields
            .iter()
            .map(|f| {
                format!(
                    "{}.{} = {}",
                    temp,
                    f.0,
                    emit_expr(&f.1, depth + 1, expected_type)
                )
            })
            .collect();
        format!(
            "([&]() {{ {} {}={{}}; {}; return {}; }}())",
            type_name,
            temp,
            inits.join("; "),
            temp
        )
    }
}

pub(crate) fn emit_if(
    cond: &IrExpr,
    then: &IrExpr,
    else_: &Option<Box<IrExpr>>,
    has_panic: bool,
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    let cond_str = emit_expr(cond, depth, expected_type);
    let then_str = emit_expr(then, depth + 1, expected_type);
    let else_str = match else_ {
        Some(e) => format!(" else {{ {}; }}", emit_expr(e, depth + 1, expected_type)),
        None => String::new(),
    };
    format!(
        "([&]() -> void {{ if ({}) {{ {}; }}{} }})()",
        cond_str, then_str, else_str
    )
}

pub(crate) fn emit_block(
    stmts: &[IrStmt],
    result: &Option<Box<IrExpr>>,
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    let ind = indent_str(depth);
    let inner = depth + 1;
    let stmt_strs: String = stmts.iter().fold(String::new(), |acc, stmt| {
        format!("{}{}", acc, emit_stmt(stmt, inner))
    });
    let result_str = match result {
        Some(expr) => format!(
            "{}return {};\n",
            indent_str(inner),
            emit_expr(expr, inner, expected_type)
        ),
        None => String::new(),
    };
    format!("([&]() {{\n{}{}{}}})()", stmt_strs, result_str, ind)
}

pub(crate) fn emit_loop_body(
    body: &[IrStmt],
    result: &Option<Box<IrExpr>>,
    depth: usize,
    expected_type: Option<&str>,
    result_prefix: &str,
) -> String {
    let stmt_strs: String = body.iter().map(|s| emit_stmt(s, depth + 1)).collect();
    let result_str = match result {
        Some(e) => format!(
            "{}{}{};\n",
            indent_str(depth + 1),
            result_prefix,
            emit_expr(e, depth + 1, expected_type)
        ),
        None => String::new(),
    };
    if stmt_strs.is_empty() && result_str.is_empty() {
        String::new()
    } else {
        format!("{{\n{}{}\n{}}}", stmt_strs, result_str, indent_str(depth))
    }
}

pub(crate) fn emit_while(
    cond: &IrExpr,
    body: &[IrStmt],
    result: &Option<Box<IrExpr>>,
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    let cond_str = emit_expr(cond, depth, expected_type);
    // A trailing expression runs each iteration for its effects; `return`
    // here would exit the wrapping lambda after the first iteration.
    let body_str = emit_loop_body(body, result, depth, expected_type, "");
    format!("([&]() {{ while ({}) {{ {} ;}}}})()", cond_str, body_str)
}

pub(crate) fn emit_loop(
    body: &[IrStmt],
    result: &Option<Box<IrExpr>>,
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    let body_str = emit_loop_body(body, result, depth, expected_type, "return ");
    format!("([&]() {{ for (;;) {{ {} ;}}}})()", body_str)
}

pub(crate) fn emit_for(
    var: &str,
    range: &IrExpr,
    body: &[IrStmt],
    result: &Option<Box<IrExpr>>,
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    let range_str = emit_expr(range, depth, expected_type);
    let body_str = emit_loop_body(body, result, depth, expected_type, "");
    format!(
        "([&]() {{ for (const auto& {} : {}) {{ {} ;}}}})()",
        var, range_str, body_str
    )
}

pub(crate) fn emit_closure_ref(id: usize) -> String {
    let env_name = format!("__closure_env_{}", id);
    let thunk_name = format!("__closure_thunk_{}", id);
    let closure = CLOSURE_DEFS.with(|c| {
        let closures = c.borrow();
        closures.get(&id).map(|cl| cl.clone())
    });
    match closure {
        Some(cl) => {
            let capture_exprs: Vec<_> = cl
                .env_fields
                .iter()
                .map(|(n, _)| format!("{},", n))
                .collect();
            let capture_init = if capture_exprs.is_empty() {
                String::new()
            } else {
                format!("{{{}}}", capture_exprs.join(" "))
            };
            let closure_ty = if cl.param_types.is_empty() {
                let ret_cxx = cxx_type(&cl.ret_type);
                format!("mvp_closure<{}>", ret_cxx)
            } else {
                let ret_cxx = cxx_type(&cl.ret_type);
                format!("mvp_closure<{}, {}>", ret_cxx, cl.param_types.join(", "))
            };
            format!(
                "({} {{ new {}{}, &{}, [](void* p) {{ delete static_cast<{}*>(p); }} }})",
                closure_ty, env_name, capture_init, thunk_name, env_name
            )
        }
        None => format!("/* missing closure {} */", id),
    }
}

pub(crate) fn emit_choose(
    var: &IrExpr,
    cases: &[IrCase],
    otherwise: &Option<Box<IrExpr>>,
    has_panic: bool,
    depth: usize,
    expected_type: Option<&str>,
) -> String {
    let var_str = emit_expr(var, depth, expected_type);
    let ind = indent_str(depth);
    let inner = depth + 1;
    let ret_suffix = match expected_type {
        Some(t) => format!(" -> {} ", t),
        None => String::new(),
    };
    let phantom_return = |t: &str| -> String { format!("return {}();", t) };
    let cast = |t: &str, e: &str| -> String { format!("static_cast<{}>({})", t, e) };
    let branch_body = |e: &IrExpr| -> String {
        if is_panic_expr(e) {
            if let Some(t) = expected_type {
                format!(
                    "{}{}; {}",
                    indent_str(inner),
                    emit_expr(e, inner, expected_type),
                    phantom_return(t)
                )
            } else {
                format!(
                    "{}{};",
                    indent_str(inner),
                    emit_expr(e, inner, expected_type)
                )
            }
        } else {
            let inner_expr = emit_expr(e, inner, expected_type);
            match expected_type {
                Some(t) => format!("{}return {};", indent_str(inner), cast(t, &inner_expr)),
                None => format!("{}return {};", indent_str(inner), inner_expr),
            }
        }
    };
    let cases_str: String = cases.iter().fold(String::new(), |acc, c| {
        let guard_str = match &c.guard {
            Some(g) => format!(" && ({})", emit_expr(g, depth, expected_type)),
            None => String::new(),
        };
        let (tag_disc, binding_names, bind_enum, bind_variant) = match &c.pattern {
            IrPattern::EnumTag {
                enum_name,
                variant,
                bindings,
            } => (
                format!("{}.__tag == {}_{}_tag()", var_str, enum_name, variant),
                bindings.clone(),
                enum_name.clone(),
                variant.clone(),
            ),
            IrPattern::Value(e) => (String::new(), Vec::new(), String::new(), String::new()),
        };
        if !tag_disc.is_empty() {
            let bind_str: String = binding_names
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    format!(
                        "{}{}const auto {} = {};\n",
                        indent_str(inner),
                        ind,
                        b,
                        enum_payload_field_ref(&var_str, &bind_enum, &bind_variant, i)
                    )
                })
                .collect();
            let body = branch_body(&c.then);
            match &c.guard {
                Some(_) => {
                    let guard_expr = emit_expr(c.guard.as_ref().unwrap(), inner, expected_type);
                    format!(
                        "{}{}if ({}) {{\n{}{}if ({}) {{\n{}{}\n{}}}\n{}}}\n",
                        acc,
                        ind,
                        tag_disc,
                        bind_str,
                        indent_str(inner),
                        guard_expr,
                        indent_str(inner),
                        body,
                        ind,
                        ind
                    )
                }
                None => {
                    format!(
                        "{}{}if ({}) {{\n{}{}\n{}}}\n",
                        acc, ind, tag_disc, bind_str, body, ind
                    )
                }
            }
        } else {
            let value_str = emit_expr(
                match &c.pattern {
                    IrPattern::Value(e) => e,
                    _ => unreachable!(),
                },
                depth,
                expected_type,
            );
            let body = branch_body(&c.then);
            format!(
                "{}{}if (({} == {}){}) {{ {} }}\n",
                acc, ind, var_str, value_str, guard_str, body
            )
        }
    });
    let otherwise_str = match otherwise {
        Some(e) if is_panic_expr(e) => {
            if let Some(t) = expected_type {
                format!(
                    "{}else {{ {}; {} }}",
                    ind,
                    emit_expr(e, inner, expected_type),
                    phantom_return(t)
                )
            } else {
                format!("{}else {{ {}; }}", ind, emit_expr(e, inner, expected_type))
            }
        }
        Some(e) => {
            let inner_expr = emit_expr(e, inner, expected_type);
            match expected_type {
                Some(t) => format!("{}else {{ return {}; }}", ind, cast(t, &inner_expr)),
                None => format!("{}else {{ return {}; }}", ind, inner_expr),
            }
        }
        None => String::new(),
    };
    format!(
        "([&](){} {{\n{}{}{}\n{}\n}}())",
        ret_suffix, cases_str, otherwise_str, ind, ind
    )
}

pub fn emit_stmt(stmt: &IrStmt, depth: usize) -> String {
    let ind = indent_str(depth);
    match stmt {
        IrStmt::Let {
            mutable,
            name,
            expr,
        } => {
            let mut_str = if *mutable { "auto " } else { "const auto " };
            format!(
                "{}{}{} = {};\n",
                ind,
                mut_str,
                name,
                emit_expr(expr, depth, None)
            )
        }
        IrStmt::LetTyped { name, typ, expr } => {
            format!(
                "{}{} {} = {};\n",
                ind,
                cxx_type(typ),
                name,
                emit_expr(expr, depth, None)
            )
        }
        IrStmt::Return(expr) => {
            format!("{}return {};\n", ind, emit_expr(expr, depth, None))
        }
        IrStmt::Expr(expr) => {
            format!("{}{};\n", ind, emit_expr(expr, depth, None))
        }
        IrStmt::Assign { name, expr } => {
            format!("{}{} = {};\n", ind, name, emit_expr(expr, depth, None))
        }
        IrStmt::FieldAssign {
            target,
            field,
            expr,
        } => {
            let target_str = emit_expr(target, depth, None);
            format!(
                "{}const_cast<std::remove_const_t<std::remove_reference_t<decltype({})>>&>({}).{} = ({});\n",
                ind,
                target_str,
                target_str,
                field,
                emit_expr(expr, depth, None)
            )
        }
        IrStmt::Empty => String::new(),
        IrStmt::If { cond, then, else_ } => emit_plain_if(cond, then, else_, depth),
        IrStmt::While { cond, body } => emit_plain_while(cond, body, depth),
        IrStmt::Loop { body } => emit_plain_loop(body, depth),
        IrStmt::For { var, range, body } => emit_plain_for(var, range, body, depth),
    }
}

pub(crate) fn emit_plain_if(
    cond: &IrExpr,
    then: &[IrStmt],
    else_: &[IrStmt],
    depth: usize,
) -> String {
    let ind = indent_str(depth);
    let inner = depth + 1;
    let cond_str = emit_expr(cond, depth, None);
    let then_str: String = then.iter().map(|s| emit_stmt(s, inner)).collect();
    let else_str = if else_.is_empty() {
        String::new()
    } else {
        let else_body: String = else_.iter().map(|s| emit_stmt(s, inner)).collect();
        format!("{}else {{\n{}{}}}\n", ind, else_body, ind)
    };
    format!("{}if ({}) {{\n{}{}}}\n", ind, cond_str, then_str, ind)
}

pub(crate) fn emit_plain_while(cond: &IrExpr, body: &[IrStmt], depth: usize) -> String {
    let ind = indent_str(depth);
    let inner = depth + 1;
    let cond_str = emit_expr(cond, depth, None);
    let body_str: String = body.iter().map(|s| emit_stmt(s, inner)).collect();
    format!("{}while ({}) {{\n{}{}}}\n", ind, cond_str, body_str, ind)
}

pub(crate) fn emit_plain_loop(body: &[IrStmt], depth: usize) -> String {
    let ind = indent_str(depth);
    let inner = depth + 1;
    let body_str: String = body.iter().map(|s| emit_stmt(s, inner)).collect();
    format!("{}for (;;) {{\n{}{}}}\n", ind, body_str, ind)
}

pub(crate) fn emit_plain_for(var: &str, range: &IrExpr, body: &[IrStmt], depth: usize) -> String {
    let ind = indent_str(depth);
    let inner = depth + 1;
    let range_str = emit_expr(range, depth, None);
    let body_str: String = body.iter().map(|s| emit_stmt(s, inner)).collect();
    format!(
        "{}for (const auto& {} : {}) {{\n{}{}}}\n",
        ind, var, range_str, body_str, ind
    )
}

pub fn emit_def(def: &IrDef, depth: usize) -> String {
    let ind = indent_str(depth);
    let inner = depth + 1;
    match def {
        IrDef::Struct {
            name,
            type_params,
            fields,
        } => emit_struct_def(name, type_params, fields, ind, inner),
        IrDef::Enum {
            name,
            type_params,
            variants,
        } => emit_enum_def(name, type_params, variants, ind, inner),
        IrDef::Func {
            name,
            type_params,
            params,
            returns,
            body_stmts,
            body_result,
            ..
        } => emit_normal_func(
            name,
            type_params,
            params,
            returns,
            body_stmts,
            body_result,
            ind,
            inner,
        ),
        IrDef::AsyncFunc {
            name,
            type_params,
            params,
            returns,
            body_stmts,
            body_result,
        } => emit_async_func(
            name,
            type_params,
            params,
            returns,
            body_stmts,
            body_result,
            ind,
            inner,
        ),
        IrDef::CFunc {
            name,
            params,
            returns,
            code,
        } => emit_cfunc(name, params, returns, code, ind),
        IrDef::Test {
            name,
            body_stmts,
            body_result,
        } => emit_test(name, body_stmts, body_result, ind, inner),
        IrDef::Impl { struct_name, impls } => emit_impl(struct_name, impls, ind),
        IrDef::Module { name, defs } => emit_module(name, defs, ind, inner),
        IrDef::Export(symbol) => String::new(),
        IrDef::Import { .. } | IrDef::ImportAs { .. } | IrDef::ImportHere { .. } => String::new(),
        IrDef::CMagical { .. } | IrDef::CIntro { .. } => String::new(),
    }
}

pub(crate) fn emit_struct_def(
    name: &str,
    type_params: &[String],
    fields: &[FieldDef],
    ind: String,
    inner: usize,
) -> String {
    let field_strs: String = fields
        .iter()
        .map(|f| format!("{}{} {};\n", indent_str(inner), cxx_type(&f.typ), f.name))
        .collect();
    let template_header = if type_params.is_empty() {
        String::new()
    } else {
        let params_str = type_params
            .iter()
            .map(|tp| format!("typename {}", tp))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}template<{}>\n", ind, params_str)
    };
    format!(
        "{}struct {} {{\n{}{}}};\n\n",
        template_header, name, field_strs, ind
    )
}

pub(crate) fn emit_enum_def(
    name: &str,
    type_params: &[String],
    variants: &[crate::ast::EnumVariant],
    ind: String,
    inner: usize,
) -> String {
    let template = if type_params.is_empty() {
        String::new()
    } else {
        format!(
            "template<{}>\n",
            type_params
                .iter()
                .map(|tp| format!("typename {}", tp))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let first_payload_idx = variants.iter().position(|v| !v.payload.is_empty());
    let mut payload_members = String::new();
    for (v_idx, v) in variants.iter().enumerate() {
        if v.payload.is_empty() {
            continue;
        }
        let fields: Vec<String> = v
            .payload
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{} field{};", cxx_type(t), i))
            .collect();
        if Some(v_idx) == first_payload_idx {
            payload_members.push_str(&format!("{}{}\n", indent_str(inner + 1), fields.join(" ")));
        } else {
            payload_members.push_str(&format!(
                "{}struct {{ {} }} {};\n",
                indent_str(inner + 1),
                fields.join(" "),
                v.name
            ));
        }
    }
    let payload_block = if payload_members.is_empty() {
        format!("{}struct {{}} __payload;\n", indent_str(inner))
    } else {
        format!(
            "{}struct {{\n{}{}}} __payload;\n",
            indent_str(inner),
            payload_members,
            indent_str(inner)
        )
    };
    let struct_str = format!(
        "{}{}struct {} {{\n{}mvp_builtin_int __tag;\n{}{}bool operator==(const {}& o) const {{ return __tag == o.__tag; }}\n{}bool operator!=(const {}& o) const {{ return __tag != o.__tag; }}\n{}}};\n\n",
        template, ind, name, indent_str(inner), payload_block, indent_str(inner), name, indent_str(inner), name, ind
    );
    let ret_ty = if type_params.is_empty() {
        name.to_string()
    } else {
        format!("{}<{}>", name, type_params.join(", "))
    };
    let mut ctors = String::new();
    for (idx, v) in variants.iter().enumerate() {
        let params: Vec<String> = v
            .payload
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{} __a{}", cxx_type(t), i))
            .collect();
        let inits: Vec<String> = (0..v.payload.len())
            .map(|i| {
                let field = if Some(idx) == first_payload_idx {
                    format!("field{}", i)
                } else {
                    format!("{}.field{}", v.name, i)
                };
                format!("v.__payload.{} = __a{};", field, i)
            })
            .collect();
        let ctor = format!(
            "{}{}inline {} {}_{}({}) {{\n{} {} v;\n{} v.__tag = {};\n{} {};\n{} return v;\n{}}}\n\n",
            template, ind,
            ret_ty,
            name,
            v.name,
            params.join(", "),
            indent_str(inner),
            ret_ty,
            indent_str(inner),
            idx,
            indent_str(inner),
            inits.join(&format!("\n{}", indent_str(inner))),
            indent_str(inner),
            ind
        );
        ctors.push_str(&ctor);
    }
    for (idx, v) in variants.iter().enumerate() {
        if v.payload.is_empty() {
            continue;
        }
        let disc = format!(
            "{}{}inline {} {}_{}() {{\n{} {} v;\n{} v.__tag = {};\n{} return v;\n{}}}\n\n",
            template,
            ind,
            ret_ty,
            name,
            v.name,
            indent_str(inner),
            ret_ty,
            indent_str(inner),
            idx,
            indent_str(inner),
            ind
        );
        ctors.push_str(&disc);
    }
    let mut tag_fns = String::new();
    for (idx, v) in variants.iter().enumerate() {
        tag_fns.push_str(&format!(
            "{}inline mvp_builtin_int {}_{}_tag() {{ return {}; }}\n\n",
            ind, name, v.name, idx
        ));
    }
    struct_str + &ctors + &tag_fns
}

pub(crate) fn emit_cfunc(
    name: &str,
    params: &[Param],
    returns: &Option<Typ>,
    code: &str,
    ind: String,
) -> String {
    let param_strs: Vec<_> = params.iter().map(cxx_param).collect();
    let ret_type = returns.as_ref().map_or("mvp_builtin_unit".into(), cxx_type);
    let signature = format!(
        "{} {}({})",
        ret_type,
        mangle_cpp_kw(name),
        param_strs.join(", ")
    );
    let args_decl = if params.is_empty() {
        String::new()
    } else {
        let arg_types: Vec<_> = params
            .iter()
            .map(|p| match p {
                Param::PRef { typ, .. } | Param::POwn { typ, .. } => cxx_type(typ),
            })
            .collect();
        let arg_names: Vec<_> = params
            .iter()
            .map(|p| match p {
                Param::PRef { name, .. } | Param::POwn { name, .. } => mangle_cpp_kw(name),
            })
            .collect();
        format!(
            "{}{} {}[] = {{ {} }};\n",
            ind,
            arg_types.join(", "),
            "args",
            arg_names.join(", ")
        )
    };
    format!(
        "{} {} {{\n{}{}{}}}\n\n",
        ind, signature, args_decl, code, ind
    )
}

pub(crate) fn emit_normal_func(
    name: &str,
    type_params: &[String],
    params: &[Param],
    returns: &Option<Typ>,
    body_stmts: &[IrStmt],
    body_result: &Option<IrExpr>,
    ind: String,
    _inner: usize,
) -> String {
    let param_strs: Vec<_> = params.iter().map(cxx_param).collect();
    let ret_type = returns.as_ref().map_or("mvp_builtin_unit".into(), cxx_type);

    let mut seen = std::collections::HashSet::new();
    let mut extra_tparams: Vec<String> = Vec::new();
    for p in params {
        let typ = match p {
            Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ,
        };
        collect_generic_params(typ, &mut seen, &mut extra_tparams);
    }
    if let Some(r) = returns {
        collect_generic_params(r, &mut seen, &mut extra_tparams);
    }
    let all_tparams: Vec<String> = {
        let mut combined: Vec<String> = type_params.to_vec();
        for tp in &extra_tparams {
            if !combined.contains(tp) {
                combined.push(tp.clone());
            }
        }
        combined
    };

    let signature = format!(
        "{} {}({})",
        ret_type,
        mangle_cpp_kw(name),
        param_strs.join(", ")
    );
    let inline_prefix = if all_tparams.is_empty() {
        "inline "
    } else {
        ""
    };
    let template_header = if all_tparams.is_empty() {
        String::new()
    } else {
        let params_str = all_tparams
            .iter()
            .map(|tp| format!("typename {}", tp))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}template<{}>\n", ind, params_str)
    };
    let body_str = if ret_type == "mvp_builtin_unit" {
        let stmt_strs: String = body_stmts.iter().map(|s| emit_stmt(s, 1)).collect();
        let ret_line = match body_result {
            Some(expr) => {
                format!(
                    "  {};\n  return mvp_builtin_void;\n",
                    emit_expr(expr, 1, None)
                )
            }
            None => "  return mvp_builtin_void;\n".into(),
        };
        format!(
            "{} {} {{\n{}{}{}}}\n\n",
            ind,
            format!("{}{}", inline_prefix, signature),
            stmt_strs,
            ret_line,
            ind
        )
    } else {
        let stmt_strs: String = body_stmts.iter().map(|s| emit_stmt(s, 1)).collect();
        let has_return = body_stmts.iter().any(|s| matches!(s, IrStmt::Return(_)));
        let ret_line = match body_result {
            Some(expr) => format!(
                "  return {};\n",
                emit_expr(expr, 1, Some(ret_type.as_str()))
            ),
            None if !has_return => format!("  return {}();\n", ret_type),
            None => String::new(),
        };
        format!(
            "{} {} {{\n{}{}{}}}\n\n",
            ind,
            format!("{}{}", inline_prefix, signature),
            stmt_strs,
            ret_line,
            ind
        )
    };
    format!("{}{}", template_header, body_str)
}

pub(crate) fn emit_async_func(
    name: &str,
    type_params: &[String],
    params: &[Param],
    returns: &Option<Typ>,
    body_stmts: &[IrStmt],
    body_result: &Option<IrExpr>,
    ind: String,
    inner: usize,
) -> String {
    let ret_type = returns
        .as_ref()
        .map_or_else(|| "mvp_future<mvp_builtin_unit>".to_string(), cxx_type);
    let inner_typ = match returns {
        Some(Typ::TFuture { of }) => (**of).clone(),
        _ => Typ::TNull,
    };
    let inner_ret = cxx_type(&inner_typ);
    let param_strs: Vec<_> = params.iter().map(cxx_param).collect();
    let signature = format!(
        "{} {}({})",
        ret_type,
        mangle_cpp_kw(name),
        param_strs.join(", ")
    );
    let template_header = if type_params.is_empty() {
        String::new()
    } else {
        let params_str = type_params
            .iter()
            .map(|tp| format!("typename {}", tp))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}template<{}>\n", ind, params_str)
    };
    let capture_list = if params.is_empty() {
        "[]".to_string()
    } else {
        let names: Vec<_> = params
            .iter()
            .map(|p| match p {
                Param::PRef { name, .. } | Param::POwn { name, .. } => mangle_cpp_kw(name),
            })
            .collect();
        format!("[{}]", names.join(", "))
    };
    let stmt_strs: String = body_stmts.iter().map(|s| emit_stmt(s, inner + 1)).collect();
    let result_str = match body_result {
        Some(expr) => format!(
            "{}return {};\n",
            indent_str(inner + 1),
            emit_expr(expr, inner + 1, Some(&inner_ret))
        ),
        None => String::new(),
    };
    let lambda = format!(
        "return mvp_async_spawn({}() -> {} {{\n{}{}}});\n",
        capture_list, inner_ret, stmt_strs, result_str
    );
    format!(
        "{}{} {{\n{}{}{}}}\n\n",
        template_header,
        signature,
        indent_str(inner),
        lambda,
        ind
    )
}

pub(crate) fn emit_test(
    name: &str,
    body_stmts: &[IrStmt],
    body_result: &Option<IrExpr>,
    ind: String,
    inner: usize,
) -> String {
    let signature = format!("mvp_builtin_int {}", name);
    let stmt_strs: String = body_stmts.iter().map(|s| emit_stmt(s, inner)).collect();
    let ret_line = match body_result {
        Some(expr) => format!(
            "{}return {};\n",
            indent_str(inner),
            emit_expr(expr, inner, Some("mvp_builtin_int"))
        ),
        None => format!("{}return mvp_builtin_void;\n", indent_str(inner)),
    };
    format!(
        "{} {} {{\n{}{}{}}}\n\n",
        ind, signature, stmt_strs, ret_line, ind
    )
}

pub(crate) fn emit_impl(struct_name: &str, impls: &[ImplExpr], ind: String) -> String {
    let mut ret = String::new();
    for impl_expr in impls {
        let op = &impl_expr.op;
        let fn_name = &impl_expr.func;
        if matches!(op, ImplOp::ImDrop) {
            continue;
        }
        let ret_typ = match op {
            ImplOp::ImAdd | ImplOp::ImSub | ImplOp::ImMul | ImplOp::ImDiv => {
                struct_name.to_string()
            }
            ImplOp::ImEq | ImplOp::ImNeq => "mvp_builtin_boolean".to_string(),
            ImplOp::ImDrop => unreachable!(),
        };
        let operator = match op {
            ImplOp::ImAdd => "+",
            ImplOp::ImSub => "-",
            ImplOp::ImMul => "*",
            ImplOp::ImDiv => "/",
            ImplOp::ImEq => "==",
            ImplOp::ImNeq => "!=",
            ImplOp::ImDrop => unreachable!(),
        };
        ret.push_str(&format!(
            "{} {} operator{}(const {}& ____a, const {}& ____b) {{ return {}(____a, ____b); }}\n\n",
            ind, ret_typ, operator, struct_name, struct_name, fn_name
        ));
    }
    ret
}

pub(crate) fn emit_module(name: &str, defs: &[IrDef], ind: String, inner: usize) -> String {
    let parts = module_parts(name);
    let ns_start: String = parts
        .iter()
        .map(|p| format!("namespace {} {{\n\n", p))
        .collect();
    let ns_end: String = parts.iter().map(|_| "}\n\n".to_string()).collect();
    let defs_str: String = defs.iter().map(|d| emit_def(d, inner)).collect();
    format!("{}{}{}", ns_start, defs_str, ns_end)
}

// ===== PROGRAM ASSEMBLY =====

pub(crate) struct ScopeParts {
    pub(crate) includes: String,
    pub(crate) defs_str: String,
    pub(crate) main_functions: String,
}

pub(crate) fn generate_with_scope(defs: &[IrDef], module: Option<&str>) -> ScopeParts {
    let mut includes = String::new();
    let mut defs_str = String::new();
    let mut main_functions = String::new();

    for (i, def) in defs.iter().enumerate() {
        match def {
            IrDef::Module {
                name,
                defs: inner_defs,
                ..
            } => {
                let parts = module_parts(name);
                let ns_start: String = parts
                    .iter()
                    .map(|p| format!("namespace {} {{\n\n", p))
                    .collect();
                let ns_end: String = parts.iter().map(|_| "}\n\n".to_string()).collect();
                let inner = generate_with_scope(inner_defs, Some(name.as_str()));
                includes.push_str(&inner.includes);
                defs_str.push_str(&ns_start);
                defs_str.push_str(&inner.defs_str);
                defs_str.push_str(&ns_end);
                main_functions.push_str(&inner.main_functions);
                break;
            }
            IrDef::Func {
                name,
                body_stmts,
                body_result,
                ..
            } if name == "main" => {
                let mvp_main_str = emit_main_func(body_stmts, body_result);
                defs_str.push_str(&mvp_main_str);
                let global_main = if let Some(name) = module {
                    format!(
                        "int main(int argc, char** argv)\n{{\n  try {{\n  {}::mvp_own_main(argc);\n  }} catch (std::exception& e) {{\n     mvp_errorlns(\"panic: \", e.what());}}\n  return 0;\n}}\n\n",
                        cxx_module(name)
                    )
                } else {
                    "int main(int argc, char** argv)\n{\n  mvp_own_main(argc);\n  return 0;\n}\n\n"
                        .into()
                };
                main_functions.push_str(&global_main);
            }
            IrDef::Import { path, .. } => includes.push_str(&cxx_include_here(path)),
            IrDef::ImportAs { path, .. } => includes.push_str(&cxx_include_path(path)),
            IrDef::ImportHere { path, .. } => includes.push_str(&cxx_include_here(path)),
            _ => defs_str.push_str(&emit_def(def, 0)),
        }
    }

    ScopeParts {
        includes,
        defs_str,
        main_functions,
    }
}

pub(crate) fn emit_main_func(body_stmts: &[IrStmt], body_result: &Option<IrExpr>) -> String {
    let signature = "mvp_builtin_unit mvp_own_main(mvp_builtin_int argc)";
    let stmt_strs: String = body_stmts
        .iter()
        .map(|s| emit_stmt(s, 1))
        .collect::<Vec<_>>()
        .join("");
    let ret_line = match body_result {
        Some(expr) => format!(
            "  {};\n  return mvp_builtin_void;\n",
            emit_expr(expr, 1, None)
        ),
        None => "  return mvp_builtin_void;\n".into(),
    };
    let mut out = String::new();
    out.push_str(signature);
    out.push_str(" {\n");
    out.push_str(&stmt_strs);
    out.push_str(&ret_line);
    out.push_str("}\n\n");
    out
}

pub(crate) fn collect_imports(defs: &[IrDef], out: &mut Vec<String>) {
    for d in defs.iter() {
        match d {
            IrDef::Import { path, .. }
            | IrDef::ImportHere { path, .. }
            | IrDef::ImportAs { path, .. } => out.push(path.clone()),
            IrDef::Module { defs: inner, .. } => collect_imports(inner, out),
            _ => {}
        }
    }
}

pub(crate) fn generate_header(defs: &[IrDef]) -> String {
    let sym = SymbolTable::build(&defs_to_ast(defs));
    let mut exported = String::new();
    collect_exported_rec(defs, &sym, &[], &mut exported);
    if exported.is_empty() {
        return String::new();
    }
    let mut includes = String::from("#include <mvp_builtin.h>\n");
    let mut import_paths = Vec::new();
    collect_imports(defs, &mut import_paths);
    for path in &import_paths {
        let inc = cxx_include_here(path);
        if !inc.is_empty() {
            includes.push_str(&inc);
        }
    }
    format!("#pragma once\n\n{}\n{}\n", includes, exported)
}

pub(crate) fn defs_to_ast(defs: &[IrDef]) -> Vec<Def> {
    defs.iter().flat_map(|d| ir_def_to_ast_rec(d)).collect()
}

pub(crate) fn ir_def_to_ast_rec(def: &IrDef) -> Vec<Def> {
    match def {
        IrDef::Module {
            name,
            defs: inner_defs,
        } => {
            let mut result = vec![Def::DModule {
                loc: Loc { line: 1, col: 1 },
                name: name.clone(),
            }];
            for inner in inner_defs {
                result.extend(ir_def_to_ast_rec(inner));
            }
            result
        }
        _ => vec![ir_def_to_ast(def)],
    }
}

pub(crate) fn ir_def_to_ast(def: &IrDef) -> Def {
    match def {
        IrDef::Struct {
            name,
            fields,
            type_params,
            ..
        } => Def::DStruct {
            loc: Loc { line: 1, col: 1 },
            name: name.clone(),
            fields: fields.clone(),
            type_params: type_params.clone(),
        },
        IrDef::Enum {
            name,
            variants,
            type_params,
            ..
        } => Def::DEnum {
            loc: Loc { line: 1, col: 1 },
            name: name.clone(),
            variants: variants.clone(),
            type_params: type_params.clone(),
        },
        IrDef::Func {
            name,
            type_params,
            params,
            returns,
            ..
        } => Def::DFunc {
            loc: Loc { line: 1, col: 1 },
            name: name.clone(),
            type_params: type_params.clone(),
            params: params.clone(),
            returns: returns.clone(),
            body: Box::new(Expr::EVoid {
                loc: Loc { line: 1, col: 1 },
            }),
            safety: Safety::Safe,
            is_async: false,
            type_bounds: vec![],
        },
        IrDef::AsyncFunc {
            name,
            type_params,
            params,
            returns,
            ..
        } => Def::DFunc {
            loc: Loc { line: 1, col: 1 },
            name: name.clone(),
            type_params: type_params.clone(),
            params: params.clone(),
            returns: returns.clone(),
            body: Box::new(Expr::EVoid {
                loc: Loc { line: 1, col: 1 },
            }),
            safety: Safety::Safe,
            is_async: true,
            type_bounds: vec![],
        },
        IrDef::CFunc {
            name,
            params,
            returns,
            ..
        } => Def::DCFuncUnsafe {
            loc: Loc { line: 1, col: 1 },
            name: name.clone(),
            params: params.clone(),
            returns: returns.clone(),
            code: String::new(),
            safety: Safety::Unsafe,
            used_c_keyword: false,
        },
        IrDef::Test { name, .. } => Def::DTest {
            loc: Loc { line: 1, col: 1 },
            name: name.clone(),
            body: Box::new(Expr::EVoid {
                loc: Loc { line: 1, col: 1 },
            }),
        },
        IrDef::Impl { struct_name, impls } => Def::DImpl {
            loc: Loc { line: 1, col: 1 },
            struct_name: struct_name.clone(),
            impls: impls.clone(),
        },
        IrDef::Module { name, .. } => Def::DModule {
            loc: Loc { line: 1, col: 1 },
            name: name.clone(),
        },
        IrDef::Export(symbol) => Def::SExport {
            loc: Loc { line: 1, col: 1 },
            symbol: symbol.clone(),
        },
        IrDef::Import { path } => Def::SImport {
            loc: Loc { line: 1, col: 1 },
            path: path.clone(),
        },
        IrDef::ImportAs { path, alias } => Def::SImportAs {
            loc: Loc { line: 1, col: 1 },
            path: path.clone(),
            alias: alias.clone(),
        },
        IrDef::ImportHere { path } => Def::SImportHere {
            loc: Loc { line: 1, col: 1 },
            path: path.clone(),
        },
        IrDef::CMagical { content } => Def::DCMagical {
            loc: Loc { line: 1, col: 1 },
            content: content.clone(),
        },
        IrDef::CIntro { content } => Def::DCIntro {
            loc: Loc { line: 1, col: 1 },
            content: content.clone(),
        },
    }
}

pub(crate) fn find_and_emit_func(defs: &[IrDef], name: &str) -> String {
    for d in defs {
        if let IrDef::Func {
            name: n,
            type_params,
            params,
            returns,
            body_stmts,
            body_result,
            ..
        } = d
        {
            if n == name {
                return emit_normal_func(
                    name,
                    type_params,
                    params,
                    returns,
                    body_stmts,
                    body_result,
                    String::new(),
                    1,
                );
            }
        }
    }
    String::new()
}

pub(crate) fn collect_exported_names(defs: &[IrDef]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for def in defs.iter() {
        match def {
            IrDef::Module {
                defs: inner_defs, ..
            } => {
                names.extend(collect_exported_names(inner_defs));
            }
            IrDef::Export(symbol) => {
                names.insert(symbol.clone());
            }
            _ => {}
        }
    }
    names
}

pub(crate) fn collect_exported_rec(
    defs: &[IrDef],
    sym: &SymbolTable,
    current_modules: &[String],
    result: &mut String,
) {
    let exported = collect_exported_names(defs);
    // First pass: emit struct and enum definitions
    for def in defs.iter() {
        match def {
            IrDef::Module {
                name,
                defs: inner_defs,
            } => {
                let mut new_modules = current_modules.to_vec();
                new_modules.push(name.clone());
                let parts = module_parts(name);
                let ns_start: String = parts
                    .iter()
                    .map(|p| format!("namespace {} {{\n\n", p))
                    .collect();
                let ns_end: String = parts.iter().map(|_| "}\n\n".to_string()).collect();
                result.push_str(&ns_start);
                collect_exported_rec(inner_defs, sym, &new_modules, result);
                result.push_str(&ns_end);
                return;
            }
            IrDef::Export(symbol) => {
                let decl = if let Some(s) = sym.lookup_struct(symbol) {
                    let field_strs: String = s
                        .fields
                        .iter()
                        .map(|f| format!("  {} {};\n", cxx_type(&f.typ), f.name))
                        .collect();
                    let template = if s.type_params.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "template<{}>\n",
                            s.type_params
                                .iter()
                                .map(|tp| format!("typename {}", tp))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    format!("{}struct {} {{\n{}}};\n\n", template, s.name, field_strs)
                } else if let Some(e) = sym.lookup_enum(symbol) {
                    emit_enum_def(&e.name, &e.type_params, &e.variants, String::new(), 0)
                } else {
                    String::new()
                };
                result.push_str(&decl);
            }
            IrDef::Enum {
                name,
                type_params,
                variants,
            } => {
                if !exported.contains(name.as_str()) {
                    let decl = emit_enum_def(name, type_params, variants, String::new(), 0);
                    result.push_str(&decl);
                }
            }
            IrDef::Struct {
                name,
                type_params,
                fields,
            } => {
                if !exported.contains(name.as_str()) {
                    let field_strs: String = fields
                        .iter()
                        .map(|f| format!("  {} {};\n", cxx_type(&f.typ), f.name))
                        .collect();
                    let template = if type_params.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "template<{}>\n",
                            type_params
                                .iter()
                                .map(|tp| format!("typename {}", tp))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    result.push_str(&format!(
                        "{}{}struct {} {{\n{}}};\n\n",
                        template, "", name, field_strs
                    ));
                }
            }
            _ => {}
        }
    }
    // Second pass: emit function definitions
    for def in defs.iter() {
        match def {
            IrDef::Func {
                name,
                type_params,
                params,
                returns,
                body_stmts,
                body_result,
                ..
            } => {
                if exported.contains(name.as_str()) {
                    let decl = find_and_emit_func(defs, name);
                    result.push_str(&decl);
                } else {
                    let has_tparams = !type_params.is_empty() || {
                        let mut seen = std::collections::HashSet::new();
                        let mut extra = Vec::new();
                        for p in params {
                            let typ = match p {
                                Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ,
                            };
                            collect_generic_params(typ, &mut seen, &mut extra);
                        }
                        if let Some(r) = returns {
                            collect_generic_params(r, &mut seen, &mut extra);
                        }
                        !extra.is_empty()
                    };
                    if has_tparams {
                        let decl = find_and_emit_func(defs, name);
                        result.push_str(&decl);
                    } else {
                        result.push_str(&cxx_func_decl(name, params, returns));
                    }
                }
            }
            IrDef::AsyncFunc {
                name,
                type_params,
                params,
                returns,
                body_stmts,
                body_result,
                ..
            } => {
                if exported.contains(name.as_str()) {
                    let decl = find_and_emit_func(defs, name);
                    result.push_str(&decl);
                } else {
                    let has_tparams = !type_params.is_empty() || {
                        let mut seen = std::collections::HashSet::new();
                        let mut extra = Vec::new();
                        for p in params {
                            let typ = match p {
                                Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ,
                            };
                            collect_generic_params(typ, &mut seen, &mut extra);
                        }
                        if let Some(r) = returns {
                            collect_generic_params(r, &mut seen, &mut extra);
                        }
                        !extra.is_empty()
                    };
                    if has_tparams {
                        let decl = find_and_emit_func(defs, name);
                        result.push_str(&decl);
                    } else {
                        result.push_str(&cxx_func_decl(name, params, returns));
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn generate_test(defs: &[IrDef]) -> String {
    let modname = defs
        .iter()
        .find_map(|d| match d {
            IrDef::Module { name, .. } => Some(name.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let modname_cxx = cxx_module(&modname);
    let header_fixed = "\
#include <mvp_test.h>
#include <mvp_builtin.h>

using namespace std;

";
    let header = format!("{}\nusing namespace {};\n", header_fixed, modname_cxx);

    let mut body = String::new();
    let mut test_names: Vec<String> = Vec::new();

    for def in defs {
        if let IrDef::Test {
            name,
            body_stmts,
            body_result,
        } = def
        {
            test_names.push(name.clone());
            let signature = format!("mvp_builtin_int {}", name);
            let stmt_strs: String = body_stmts.iter().map(|s| emit_stmt(s, 1)).collect();
            let ret_line = match body_result {
                Some(expr) => format!(
                    "  return {};\n",
                    emit_expr(expr, 1, Some("mvp_builtin_int"))
                ),
                None => "  return mvp_builtin_void;\n".into(),
            };
            body.push_str(&format!(
                "{} {{\n{}{}{}}}\n\n",
                signature, stmt_strs, ret_line, ""
            ));
        }
    }

    if test_names.is_empty() {
        return String::new();
    }

    let test_array: String = test_names
        .iter()
        .map(|n| format!("    {{\"{}\", {}}},\n", n, n))
        .collect();

    format!(
        "{}{}\nint main() {{\n    MvpTest tests[] = {{\n{}}};\n    return mvp_run_tests(tests, sizeof(tests) / sizeof(tests[0]));\n}}\n",
        header, body, test_array
    )
}

// ===== ENTRY POINT =====
