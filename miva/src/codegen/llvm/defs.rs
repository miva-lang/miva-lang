use super::*;

pub(crate) fn gen_stmt(stmt: &Stmt, ctx: &mut LlvmCtx, body: &mut String) {
    match stmt {
        Stmt::SLet { name, mutable: _, expr, .. } => {
            let val = gen_expr(expr, ctx, body);
            let (addr, reload) = ctx.declare_var(name);
            body.push_str(&format!("  %{} = alloca i64, align 8\n", addr));
            body.push_str(&format!("  store i64 {}, ptr %{}, align 8\n", val, addr));
            body.push_str(&format!("  %{} = load i64, ptr %{}, align 8\n", reload, addr));
            if binding_is_string(expr, ctx) {
                ctx.string_regs.insert(reload);
            }
            // An untyped binding of a lambda is a closure-typed variable.
            if let Expr::ELambda { params, ret, .. } = &**expr {
                ctx.var_types.insert(name.clone(), Typ::TFunc {
                    params: params.iter().map(|p| match p {
                        Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ.clone(),
                    }).collect(),
                    returns: Box::new(ret.clone()),
                });
            }
            record_string_payloads(name, expr, ctx);
        }
        Stmt::SLetTyped { name, typ, expr, .. } => {
            let val = gen_expr(expr, ctx, body);
            let (addr, reload) = ctx.declare_var(name);
            body.push_str(&format!("  %{} = alloca i64, align 8\n", addr));
            body.push_str(&format!("  store i64 {}, ptr %{}, align 8\n", val, addr));
            body.push_str(&format!("  %{} = load i64, ptr %{}, align 8\n", reload, addr));
            if binding_is_string(expr, ctx) {
                ctx.string_regs.insert(reload);
            }
            ctx.var_types.insert(name.clone(), typ.clone());
            if let Some(idxs) = enum_string_payloads_from_typ(typ, &ctx.enum_defs) {
                if !idxs.is_empty() {
                    ctx.string_payloads.insert(name.clone(), idxs);
                }
            }
            record_string_payloads(name, expr, ctx);
        }
        Stmt::SAssign { name, expr, .. } => {
            let val = gen_expr(expr, ctx, body);
            let addr = ctx.get_var_addr(name);
            body.push_str(&format!("  store i64 {}, ptr %{}, align 8\n", val, addr));
            // Stay inside the `s.r.N` namespace so re-declares of `s` (SLet
            // via `var_seq`) and other fresh loads (`emit_fresh_loads`) can't
            // land on the same suffix. `tmp_counter` would collide.
            let count = ctx.var_seq.entry(name.to_string()).or_insert(0);
            let reload_name = format!("{}.r.{}", name, count);
            *count += 1;
            body.push_str(&format!("  %{} = load i64, ptr %{}, align 8\n", reload_name, addr));
            ctx.var_reloads.insert(name.clone(), reload_name.clone());
            if binding_is_string(expr, ctx) {
                ctx.string_regs.insert(reload_name);
            }
            record_string_payloads(name, expr, ctx);
        }
        Stmt::SReturn { expr, .. } => {
            let val = gen_expr(expr, ctx, body);
            body.push_str(&format!("  ret i64 {}\n", val));
        }
        Stmt::SFieldAssign { target, field, expr, .. } => {
            // `target.field = expr` — LLVM backend field write. Compute the
            // base struct pointer (i64), GEP to the field's offset, store the
            // value. Field indices come from the per-module struct field map
            // in `ctx` (same source as EFieldAccess reads).
            let base = gen_expr(target, ctx, body);
            let val = gen_expr(expr, ctx, body);
            let field_idx = ctx.field_idx.get(field.as_str()).copied().unwrap_or(0);
            let ptr_tmp = ctx.gen_tmp("fa");
            body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", ptr_tmp, base));
            let gep = ctx.gen_tmp("g");
            body.push_str(&format!("  {} = getelementptr i64, ptr {}, i64 {}\n", gep, ptr_tmp, field_idx));
            body.push_str(&format!("  store i64 {}, ptr {}\n", val, gep));
        }
        Stmt::SExpr { expr, .. } => { gen_expr(expr, ctx, body); }
        Stmt::SCIntro { content, .. } => { body.push_str(&format!("  ; {}\n", content)); }
        Stmt::SEmpty { .. } => {}
    }
}

pub(crate) fn gen_func_def(
    name: &str, _type_params: &[String], params: &[Param], _returns: &Option<Typ>,
    body: &Expr, module: Option<&str>, struct_field_map: &HashMap<String, HashMap<String, usize>>, struct_field_types: &HashMap<String, HashMap<String, Typ>>, func_sigs: &HashMap<String, crate::codegen::FuncSig>,
    enum_defs: &HashMap<String, (Vec<String>, HashMap<String, Vec<Typ>>)>,
    is_async: bool,
) -> String {
    let global_name = make_global_name(module, name);
    let mut ctx = LlvmCtx::with_module_and_fields(module, struct_field_map.clone(), struct_field_types.clone(), enum_defs.clone()).with_func_sigs(func_sigs);
    ctx.indent = 1;
    let mut body_prefix = String::new();
    let param_strs: Vec<String>;
    if is_async {
        // Async functions take a single i64 that is a pointer to a heap struct
        // holding the packed arguments. The runtime spawns them on a thread.
        param_strs = vec!["i64 %args".to_string()];
        body_prefix.push_str("  %args_ptr = inttoptr i64 %args to ptr\n");
        for (i, p) in params.iter().enumerate() {
            let pname = match p { Param::PRef { name, .. } | Param::POwn { name, .. } => name.as_str() };
            let (addr, reload) = ctx.declare_var(pname);
            let gep = ctx.gen_tmp("ag");
            body_prefix.push_str(&format!("  {} = getelementptr i64, ptr %args_ptr, i64 {}\n", gep, i));
            let val = ctx.gen_tmp("av");
            body_prefix.push_str(&format!("  {} = load i64, ptr {}\n", val, gep));
            body_prefix.push_str(&format!("  %{} = alloca i64, align 8\n", addr));
            body_prefix.push_str(&format!("  store i64 {}, ptr %{}, align 8\n", val, addr));
            body_prefix.push_str(&format!("  %{} = load i64, ptr %{}, align 8\n", reload, addr));
        }
    } else {
        param_strs = params.iter().map(|p| {
            let pname = match p { Param::PRef { name, .. } | Param::POwn { name, .. } => name.as_str() };
            format!("i64 %{}", pname)
        }).collect();
        for p in params {
            let (pname, ptyp) = match p {
                Param::PRef { name, typ, .. } | Param::POwn { name, typ, .. } => (name.as_str(), typ),
            };
            let (addr, reload) = ctx.declare_var(pname);
            body_prefix.push_str(&format!("  %{} = alloca i64, align 8\n", addr));
            body_prefix.push_str(&format!("  store i64 %{}, ptr %{}, align 8\n", pname, addr));
            body_prefix.push_str(&format!("  %{} = load i64, ptr %{}, align 8\n", reload, addr));
            if let Some(idxs) = enum_string_payloads_from_typ(ptyp, &ctx.enum_defs) {
                if !idxs.is_empty() {
                    ctx.string_payloads.insert(pname.to_string(), idxs);
                }
            }
            ctx.var_types.insert(pname.to_string(), ptyp.clone());
        }
    }
    let mut body_str = String::new();
    // Implicit return: for non-void functions whose body is a block with no
    // explicit result expression, the last SExpr is treated as the block value
    // (matching CXX's take_last_expr in cxx_func_inner).  Without this a
    // function like `{ ptr_alloc(n); }` returns 0 instead of the allocation.
    let ret_val = if _returns.is_some() {
        if let Expr::EBlock { stmts, result: None, .. } = body {
            if let Some((Stmt::SExpr { expr, .. }, leading)) = stmts.split_last() {
                for s in leading {
                    gen_stmt(s, &mut ctx, &mut body_str);
                }
                gen_expr(expr.as_ref(), &mut ctx, &mut body_str)
            } else {
                gen_expr(body, &mut ctx, &mut body_str)
            }
        } else {
            gen_expr(body, &mut ctx, &mut body_str)
        }
    } else {
        gen_expr(body, &mut ctx, &mut body_str)
    };
    let mut out = String::new();
    out.push_str(&ctx.string_constants);
    out.push_str(&format!("define i64 @{}({}) {{\n", global_name, param_strs.join(", ")));
    out.push_str("entry:\n");
    out.push_str(&body_prefix);
    out.push_str(&body_str);
    out.push_str(&format!("  ret i64 {}\n", ret_val));
    out.push_str("}\n\n");
    out
}

/// Lower a lambda (closure) to an LLVM thunk function plus a closure value.
///
/// Emits a thunk `define i64 @__closure_thunk_N(ptr %env, i64 %arg0, ...)` that
/// reconstructs the captured variables from `%env` and runs the lambda body,
/// appending it to the module-level `CLOSURE_THUNK_DEFS` buffer. Returns the
/// thunk's global name so the call site can build the closure value (a pointer
/// to a `{ i64 env, i64 fn }` struct).
pub(crate) fn gen_closure_thunk(
    captures: &[(String, Typ)],
    params: &[Param],
    ret: &Typ,
    body: &Expr,
    struct_field_map: &HashMap<String, HashMap<String, usize>>,
    struct_field_types: &HashMap<String, HashMap<String, Typ>>,
    func_sigs: &HashMap<String, crate::codegen::FuncSig>,
    enum_defs: &HashMap<String, (Vec<String>, HashMap<String, Vec<Typ>>)>,
) -> String {
    let id = CLOSURE_THUNK_ID.fetch_add(1, Ordering::Relaxed);
    let thunk_name = format!("__closure_thunk_{}", id);

    let mut ctx = LlvmCtx::with_module_and_fields(None, struct_field_map.clone(), struct_field_types.clone(), enum_defs.clone())
        .with_func_sigs(func_sigs);
    ctx.indent = 1;

    let mut body_prefix = String::new();

    // Load captures from %env (a pointer to a heap struct of i64 values).
    for (i, (cap_name, cap_typ)) in captures.iter().enumerate() {
        let (addr, reload) = ctx.declare_var(cap_name);
        let gep = ctx.gen_tmp("cgep");
        body_prefix.push_str(&format!("  {} = getelementptr i64, ptr %env, i64 {}\n", gep, i));
        let loaded = ctx.gen_tmp("cload");
        body_prefix.push_str(&format!("  {} = load i64, ptr {}\n", loaded, gep));
        body_prefix.push_str(&format!("  %{} = alloca i64, align 8\n", addr));
        body_prefix.push_str(&format!("  store i64 {}, ptr %{}, align 8\n", loaded, addr));
        body_prefix.push_str(&format!("  %{} = load i64, ptr %{}, align 8\n", reload, addr));
        ctx.var_types.insert(cap_name.clone(), cap_typ.clone());
    }

    // Declare the lambda's own parameters (env, then arg0, arg1, ...).
    let param_strs: Vec<String> = std::iter::once("ptr %env".to_string())
        .chain(params.iter().enumerate().map(|(i, _)| format!("i64 %arg{}", i)))
        .collect();
    for (i, p) in params.iter().enumerate() {
        let (pname, ptyp) = match p {
            Param::PRef { name, typ, .. } | Param::POwn { name, typ, .. } => (name.as_str(), typ),
        };
        let (addr, reload) = ctx.declare_var(pname);
        body_prefix.push_str(&format!("  %{} = alloca i64, align 8\n", addr));
        body_prefix.push_str(&format!("  store i64 %arg{}, ptr %{}, align 8\n", i, addr));
        body_prefix.push_str(&format!("  %{} = load i64, ptr %{}, align 8\n", reload, addr));
        ctx.var_types.insert(pname.to_string(), ptyp.clone());
    }

    let mut body_str = String::new();
    let ret_val = if let Expr::EBlock { stmts, result: None, .. } = body {
        if let Some((Stmt::SExpr { expr, .. }, leading)) = stmts.split_last() {
            for s in leading {
                gen_stmt(s, &mut ctx, &mut body_str);
            }
            gen_expr(expr.as_ref(), &mut ctx, &mut body_str)
        } else {
            gen_expr(body, &mut ctx, &mut body_str)
        }
    } else {
        gen_expr(body, &mut ctx, &mut body_str)
    };

    let mut out = String::new();
    out.push_str(&ctx.string_constants);
    out.push_str(&format!("define i64 @{} ({}) {{\n", thunk_name, param_strs.join(", ")));
    out.push_str("entry:\n");
    out.push_str(&body_prefix);
    out.push_str(&body_str);
    out.push_str(&format!("  ret i64 {}\n", ret_val));
    out.push_str("}\n\n");

    if let Ok(mut guard) = CLOSURE_THUNK_DEFS.lock() {
        guard.get_or_insert_with(String::new).push_str(&out);
    }
    let _ = ret;
    thunk_name
}

pub(crate) fn gen_main_func(body_expr: &Expr, struct_field_map: &HashMap<String, HashMap<String, usize>>, struct_field_types: &HashMap<String, HashMap<String, Typ>>, func_sigs: &HashMap<String, crate::codegen::FuncSig>, enum_defs: &HashMap<String, (Vec<String>, HashMap<String, Vec<Typ>>)>) -> String {
    let mut ctx = LlvmCtx::with_module_and_fields(None, struct_field_map.clone(), struct_field_types.clone(), enum_defs.clone()).with_func_sigs(func_sigs);
    ctx.indent = 1;
    let (argc_addr, _argc_reload) = ctx.declare_var("argc");
    let mut body = String::new();
    body.push_str(&format!("  %{} = alloca i64, align 8\n", argc_addr));
    body.push_str(&format!("  store i64 %argc, ptr %{}, align 8\n", argc_addr));
    let ret_val = gen_expr(body_expr, &mut ctx, &mut body);
    let mut out = String::new();
    out.push_str(&ctx.string_constants);
    out.push_str("define i64 @mvp_own_main(i64 %argc) {\n");
    out.push_str("entry:\n");
    out.push_str(&body);
    out.push_str(&format!("  ret i64 {}\n", ret_val));
    out.push_str("}\n\n");
    out.push_str("define i32 @main(i32 %argc, ptr %argv) {\n");
    out.push_str("entry:\n");
    out.push_str("  %ext = sext i32 %argc to i64\n");
    out.push_str("  %ret = call i64 @mvp_own_main(i64 %ext)\n");
    out.push_str("  ret i32 0\n");
    out.push_str("}\n\n");
    out
}

pub(crate) fn gen_cfunc(name: &str, params: &[Param], returns: &Option<Typ>, code: &str) -> String {
    let ret_type = match returns {
        Some(typ) => match typ { Typ::TFloat64 => "double", Typ::TFloat32 => "float", Typ::TBool | Typ::TChar => "i8", _ => "i64" },
        None => "i64",
    };
    let param_strs: Vec<String> = params.iter().map(|p| {
        match p { Param::PRef { typ, .. } | Param::POwn { typ, .. } => {
            match typ { Typ::TFloat64 => "double".to_string(), Typ::TFloat32 => "float".to_string(), Typ::TBool | Typ::TChar => "i8".to_string(), _ => "i64".to_string() }
        }}
    }).collect();
    let mut out = String::new();
    out.push_str(&format!("define {} @{}({}) {{\n", ret_type, name, param_strs.join(", ")));
    out.push_str("entry:\n");
    out.push_str(&format!("  %args = alloca [{} x %MivaValue], align 8\n", params.len()));
    out.push_str(&format!("  %args_ptr = getelementptr [{} x %MivaValue], ptr %args, i64 0, i64 0\n", params.len()));
    for (i, p) in params.iter().enumerate() {
        let arg_reg = if params.len() == 1 {
            "%0".to_string()
        } else {
            format!("%{}", i)
        };
        match p {
            Param::PRef { typ, .. } | Param::POwn { typ, .. } => {
                match typ {
                    Typ::TInt => {
                        out.push_str(&format!("  %arg{}_gep = getelementptr %MivaValue, ptr %args_ptr, i64 {}, i32 0\n", i, i));
                        out.push_str(&format!("  store i64 0, ptr %arg{}_gep\n", i));
                        out.push_str(&format!("  %arg{}_data = getelementptr %MivaValue, ptr %args_ptr, i64 {}, i32 1\n", i, i));
                        out.push_str(&format!("  store i64 {}, ptr %arg{}_data\n", arg_reg, i));
                    }
                    Typ::TFloat64 | Typ::TFloat32 => {
                        out.push_str(&format!("  %arg{}_gep = getelementptr %MivaValue, ptr %args_ptr, i64 {}, i32 0\n", i, i));
                        out.push_str(&format!("  store i64 1, ptr %arg{}_gep\n", i));
                        out.push_str(&format!("  %arg{}_bits = bitcast double {} to i64\n", i, arg_reg));
                        out.push_str(&format!("  %arg{}_data = getelementptr %MivaValue, ptr %args_ptr, i64 {}, i32 1\n", i, i));
                        out.push_str(&format!("  store i64 %arg{}_bits, ptr %arg{}_data\n", i, i));
                    }
                    Typ::TBool => {
                        out.push_str(&format!("  %arg{}_gep = getelementptr %MivaValue, ptr %args_ptr, i64 {}, i32 0\n", i, i));
                        out.push_str(&format!("  store i64 2, ptr %arg{}_gep\n", i));
                        out.push_str(&format!("  %arg{}_data = getelementptr %MivaValue, ptr %args_ptr, i64 {}, i32 1\n", i, i));
                        out.push_str(&format!("  store i64 {}, ptr %arg{}_data\n", arg_reg, i));
                    }
                    _ => {
                        // String/other: treat as opaque pointer
                        out.push_str(&format!("  %arg{}_gep = getelementptr %MivaValue, ptr %args_ptr, i64 {}, i32 0\n", i, i));
                        out.push_str(&format!("  store i64 3, ptr %arg{}_gep\n", i));
                        out.push_str(&format!("  %arg{}_cstr_ptr = inttoptr i64 {} to ptr\n", i, arg_reg));
                        out.push_str(&format!("  %arg{}_cstr = call ptr @miva_string_c_str(ptr %arg{}_cstr_ptr)\n", i, i));
                        out.push_str(&format!("  %arg{}_data = getelementptr %MivaValue, ptr %args_ptr, i64 {}, i32 1\n", i, i));
                        out.push_str(&format!("  store ptr %arg{}_cstr, ptr %arg{}_data\n", i, i));
                    }
                }
            }
        }
    }
    out.push_str(&format!("  %result = call %MivaValue @miva_host_{}(ptr %args_ptr, i32 {})\n", name, params.len()));
    // Extract result based on return type
    match returns {
        Some(Typ::TString) => {
            out.push_str("  %result_s = extractvalue %MivaValue %result, 1\n");
            out.push_str("  %result_ptr = inttoptr i64 %result_s to ptr\n");
            out.push_str("  %result_str = call ptr @miva_string_from_cstr(ptr %result_ptr)\n");
            out.push_str("  %result_int = ptrtoint ptr %result_str to i64\n");
            out.push_str("  ret i64 %result_int\n");
        }
        Some(Typ::TBool) => {
            out.push_str("  %result_i = extractvalue %MivaValue %result, 1\n");
            out.push_str("  ret i64 %result_i\n");
        }
        Some(Typ::TFloat64 | Typ::TFloat32) => {
            out.push_str("  %result_bits = extractvalue %MivaValue %result, 1\n");
            out.push_str("  %result_f = bitcast i64 %result_bits to double\n");
            out.push_str("  ret double %result_f\n");
        }
        _ => {
            out.push_str("  %result_i = extractvalue %MivaValue %result, 1\n");
            out.push_str("  ret i64 %result_i\n");
        }
    }
    out.push_str("}\n\n");
    out
}

pub(crate) fn llvm_enum_def(name: &str, variants: &[crate::ast::EnumVariant]) -> String {
    let max_fields = variants.iter().map(|v| v.payload.len()).max().unwrap_or(0);
    let mut out = String::new();
    for (idx, v) in variants.iter().enumerate() {
        let nargs = v.payload.len();
        let params: Vec<String> = (0..nargs).map(|i| format!("i64 %arg{}", i)).collect();
        out.push_str(&format!("define i64 @{}_{}({}) {{\n", name, v.name, params.join(", ")));
        out.push_str("entry:\n");
        out.push_str(&format!("  %p = call ptr @miva_alloc(i64 {})\n", (max_fields + 1) * 8));
        out.push_str(&format!("  %tg = getelementptr i64, ptr %p, i64 0\n  store i64 {}, ptr %tg\n", idx));
        for i in 0..nargs {
            out.push_str(&format!(
                "  %f{} = getelementptr i64, ptr %p, i64 {}\n  store i64 %arg{}, ptr %f{}\n",
                i, i + 1, i, i
            ));
        }
        out.push_str("  %r = ptrtoint ptr %p to i64\n  ret i64 %r\n}\n\n");
        // Unit constructor used for enum discriminants: `Shape.Circle` in a
        // `when (Shape.Circle)` pattern evaluates to a full enum value (tag
        // only, payload zero-initialized) so it can be compared tag-wise with
        // the scrutinee.
        out.push_str(&format!("define i64 @{}_{}_unit() {{\n", name, v.name));
        out.push_str("entry:\n");
        out.push_str(&format!("  %p = call ptr @miva_alloc(i64 {})\n", (max_fields + 1) * 8));
        out.push_str(&format!("  %tg = getelementptr i64, ptr %p, i64 0\n  store i64 {}, ptr %tg\n", idx));
        out.push_str("  %r = ptrtoint ptr %p to i64\n  ret i64 %r\n}\n\n");
        out.push_str(&format!("define i64 @{}_{}_tag() {{\nentry:\n  ret i64 {}\n}}\n\n", name, v.name, idx));
    }
    out
}

pub(crate) fn gen_impl(_struct_name: &str, impls: &[ImplExpr]) -> String {
    let mut out = String::new();
    for impl_expr in impls { out.push_str(&format!("; impl operator for {:?} (user function {})\n", impl_expr.op, impl_expr.func)); }
    out
}

pub(crate) fn generate_with_scope(defs: &[Def], module: Option<&str>, struct_field_map: &HashMap<String, HashMap<String, usize>>, struct_field_types: &HashMap<String, HashMap<String, Typ>>, func_sigs: &HashMap<String, crate::codegen::FuncSig>) -> (String, String, String, HashSet<String>) {
    let mut struct_defs = String::new();
    let mut defs_str = String::new();
    let mut main_functions = String::new();
    let mut defined = HashSet::new();

    let mut enum_defs: HashMap<String, (Vec<String>, HashMap<String, Vec<Typ>>)> = HashMap::new();
    for def in defs {
        if let Def::DEnum { name, type_params, variants, .. } = def {
            let mut variant_map = HashMap::new();
            for v in variants {
                variant_map.insert(v.name.clone(), v.payload.clone());
            }
            enum_defs.insert(name.clone(), (type_params.clone(), variant_map));
        }
    }

    for def in defs {
        match def {
            Def::DFunc { name, type_params, params, returns, body, .. } if name == "main" => main_functions.push_str(&gen_main_func(body, struct_field_map, struct_field_types, func_sigs, &enum_defs)),
            Def::DCFuncUnsafe { name, params, returns, code, .. } => {
                let global_name = make_global_name(module, name.as_str());
                defined.insert(global_name);
                defs_str.push_str(&gen_cfunc(name, params, returns, code));
            }
            Def::DFunc { name, type_params, params, returns, body, is_async, .. } => {
                let global_name = make_global_name(module, name.as_str());
                defined.insert(global_name);
                defs_str.push_str(&gen_func_def(name, type_params, params, returns, body, module, struct_field_map, struct_field_types, func_sigs, &enum_defs, *is_async));
            }
            Def::DEnum { name, variants, .. } => {
                struct_defs.push_str(&llvm_enum_def(name, variants));
            }
            Def::DImpl { struct_name, impls, .. } => defs_str.push_str(&gen_impl(struct_name, impls)),
            Def::DModule { name, .. } => {
                let inner = generate_with_scope(&defs[1..], Some(name.as_str()), struct_field_map, struct_field_types, func_sigs);
                struct_defs.push_str(&inner.0); defs_str.push_str(&inner.1); main_functions.push_str(&inner.2);
                defined.extend(inner.3);
                break;
            }
            _ => {}
        }
    }
    (struct_defs, defs_str, main_functions, defined)
}

pub(crate) fn generate_test(defs: &[Def]) -> String {
    let mut test_ir = String::new();
    for def in defs { if let Def::DTest { name, .. } = def { test_ir.push_str(&format!("define i64 @{}() {{\nentry:\n  ret i64 0\n}}\n\n", name)); } }
    test_ir
}

pub(crate) fn generate_bridge(_defs: &[Def]) -> String {
    let mut bridge = String::new();
    bridge.push_str("#include <string>\n#include <vector>\n#include <thread>\n#include <mutex>\n#include <condition_variable>\n#include <cstdint>\n#include <cstdlib>\n#include <cstdio>\n#include <mvp_builtin.h>\n\nextern \"C\" {\n");
    bridge.push_str("void miva_print(void* s) { auto& str = *(std::string*)s; mvp_print(str); }\n");
    bridge.push_str("void miva_println(void* s) { auto& str = *(std::string*)s; mvp_println(str); }\n");
    bridge.push_str("void miva_prints(void* s) { auto& str = *(std::string*)s; mvp_prints(str); }\n");
    bridge.push_str("void miva_printlns(void* s) { auto& str = *(std::string*)s; mvp_printlns(str); }\n");
    bridge.push_str("void miva_error(void* s) { auto& str = *(std::string*)s; mvp_error(str); }\n");
    bridge.push_str("void miva_errors(void* s) { auto& str = *(std::string*)s; mvp_errors(str); }\n");
    bridge.push_str("void miva_errorln(void* s) { auto& str = *(std::string*)s; mvp_errorln(str); }\n");
    bridge.push_str("void miva_errorlns(void* s) { auto& str = *(std::string*)s; mvp_errorlns(str); }\n");
    bridge.push_str("void miva_exit(int64_t c) { mvp_exit(c); }\n");
    bridge.push_str("void miva_abort() { mvp_abort(); }\n");
    bridge.push_str("void miva_panic(void* s) { auto& str = *(std::string*)s; mvp_panic(str); }\n");
    bridge.push_str("void* miva_string_concat(void* a, void* b) { auto r = mvp_string_concat(*(std::string*)a, *(std::string*)b); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_string_parse(void* s) { return mvp_string_parse(*(std::string*)s); }\n");
    bridge.push_str("int64_t miva_string_length(void* s) { return mvp_string_length(*(std::string*)s); }\n");
    bridge.push_str("void* miva_string_make(void* init, int64_t size) { return new std::string(mvp_string_make(*(std::string*)init, size)); }\n");
    bridge.push_str("void* miva_string_from_int(int64_t v) { return new std::string(mvp_to_string(v)); }\n");
    bridge.push_str("void* miva_string_from_float(double v) { return new std::string(mvp_to_string(v)); }\n");
    bridge.push_str("void* miva_string_from_bool(int8_t v) { return new std::string(mvp_to_string(v)); }\n");
    bridge.push_str("void* miva_string_from_str(const char* s) { return new std::string(s); }\n");
    bridge.push_str("void* miva_string_c_str(void* s) { return (void*)strdup(((std::string*)s)->c_str()); }\n");
    bridge.push_str("void* miva_string_from_cstr(char* s) { auto* r = new std::string(s); free(s); return r; }\n");
    bridge.push_str("void miva_box_new_int(void** out, int64_t v) { *out = new mvp_builtin_box<mvp_builtin_int>(v); }\n");
    bridge.push_str("void miva_box_new_float(void** out, double v) { *out = new mvp_builtin_box<mvp_builtin_float>(v); }\n");
    bridge.push_str("void miva_box_new_bool(void** out, int8_t v) { *out = new mvp_builtin_box<mvp_builtin_boolean>(v); }\n");
    bridge.push_str("void miva_box_new_byte(void** out, int8_t v) { *out = new mvp_builtin_box<mvp_builtin_byte>(v); }\n");
    bridge.push_str("void miva_box_new_string(void** out, void* s) { *out = s; }\n");
    bridge.push_str("int64_t miva_box_deref_int(void* b) { return **(mvp_builtin_box<mvp_builtin_int>*)b; }\n");
    bridge.push_str("double miva_box_deref_float(void* b) { return **(mvp_builtin_box<mvp_builtin_float>*)b; }\n");
    bridge.push_str("int8_t miva_box_deref_bool(void* b) { return **(mvp_builtin_box<mvp_builtin_boolean>*)b; }\n");
    bridge.push_str("int8_t miva_box_deref_byte(void* b) { return **(mvp_builtin_box<mvp_builtin_byte>*)b; }\n");
    bridge.push_str("void miva_box_deref_string(void* out, void* b) { *(std::string*)out = **(mvp_builtin_box<mvp_builtin_string>*)b; }\n");
    bridge.push_str("void miva_range(void** out, int64_t start, int64_t end) { *out = new std::vector<mvp_builtin_int>(mvp_range(start,end)); }\n");
    bridge.push_str("void* miva_alloc(int64_t s) { return mvp_alloc(s); }\n");
    bridge.push_str("void* miva_realloc(void* p, int64_t s) { return mvp_realloc(p,s); }\n");
    bridge.push_str("void miva_free(void* p) { mvp_free(p); }\n");
    bridge.push_str("void* miva_ptr_offset(void* p, int64_t n) { return mvp_ptr_offset(p, n); }\n");
    bridge.push_str("int64_t miva_json_parse(void* s) { auto& str = *(std::string*)s; return (int64_t)(intptr_t)mvp_json_parse(str); }\n");
    bridge.push_str("int64_t miva_json_kind(int64_t v) { return mvp_json_kind((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_json_bool(int64_t v) { return mvp_json_bool((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_json_number(int64_t v) { double d = mvp_json_number((void*)(intptr_t)v); int64_t r; memcpy(&r, &d, 8); return r; }\n");
    bridge.push_str("void* miva_json_string(int64_t v) { auto r = mvp_json_string((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_json_array_len(int64_t v) { return mvp_json_array_len((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_json_array_get(int64_t v, int64_t i) { return (int64_t)(intptr_t)mvp_json_array_get((void*)(intptr_t)v, i); }\n");
    bridge.push_str("int64_t miva_json_object_len(int64_t v) { return mvp_json_object_len((void*)(intptr_t)v); }\n");
    bridge.push_str("void* miva_json_object_key(int64_t v, int64_t i) { auto r = mvp_json_object_key((void*)(intptr_t)v, i); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_json_object_get(int64_t v, int64_t i) { return (int64_t)(intptr_t)mvp_json_object_get((void*)(intptr_t)v, i); }\n");
    bridge.push_str("int64_t miva_json_object_find(int64_t v, void* key) { auto& k = *(std::string*)key; return (int64_t)(intptr_t)mvp_json_object_find((void*)(intptr_t)v, k); }\n");
    bridge.push_str("void miva_json_free(int64_t v) { mvp_json_free((void*)(intptr_t)v); }\n");
    bridge.push_str("void* miva_json_stringify(int64_t v) { auto r = mvp_json_stringify((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_xml_parse(void* s) { auto& str = *(std::string*)s; return (int64_t)(intptr_t)mvp_xml_parse(str); }\n");
    bridge.push_str("int64_t miva_xml_kind(int64_t v) { return mvp_xml_kind((void*)(intptr_t)v); }\n");
    bridge.push_str("void* miva_xml_tag(int64_t v) { auto r = mvp_xml_tag((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_xml_attr_count(int64_t v) { return mvp_xml_attr_count((void*)(intptr_t)v); }\n");
    bridge.push_str("void* miva_xml_attr_name(int64_t v, int64_t i) { auto r = mvp_xml_attr_name((void*)(intptr_t)v, i); return new std::string(std::move(r)); }\n");
    bridge.push_str("void* miva_xml_attr_value(int64_t v, int64_t i) { auto r = mvp_xml_attr_value((void*)(intptr_t)v, i); return new std::string(std::move(r)); }\n");
    bridge.push_str("void* miva_xml_attr_find(int64_t v, void* key) { auto& k = *(std::string*)key; auto r = mvp_xml_attr_find((void*)(intptr_t)v, k); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_xml_child_count(int64_t v) { return mvp_xml_child_count((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_xml_child_get(int64_t v, int64_t i) { return (int64_t)(intptr_t)mvp_xml_child_get((void*)(intptr_t)v, i); }\n");
    bridge.push_str("void* miva_xml_text(int64_t v) { auto r = mvp_xml_text((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("void* miva_xml_comment(int64_t v) { auto r = mvp_xml_comment((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("void* miva_xml_cdata(int64_t v) { auto r = mvp_xml_cdata((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("void* miva_xml_pi_target(int64_t v) { auto r = mvp_xml_pi_target((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("void* miva_xml_pi_data(int64_t v) { auto r = mvp_xml_pi_data((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("void* miva_xml_stringify(int64_t v) { auto r = mvp_xml_stringify((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("void miva_xml_free(int64_t v) { mvp_xml_free((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_toml_parse(void* s) { auto& str = *(std::string*)s; return (int64_t)(intptr_t)mvp_toml_parse(str); }\n");
    bridge.push_str("int64_t miva_toml_kind(int64_t v) { return mvp_toml_kind((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_toml_bool(int64_t v) { return mvp_toml_bool((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_toml_number(int64_t v) { double d = mvp_toml_number((void*)(intptr_t)v); int64_t r; memcpy(&r, &d, 8); return r; }\n");
    bridge.push_str("void* miva_toml_string(int64_t v) { auto r = mvp_toml_string((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_toml_array_len(int64_t v) { return mvp_toml_array_len((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_toml_array_get(int64_t v, int64_t i) { return (int64_t)(intptr_t)mvp_toml_array_get((void*)(intptr_t)v, i); }\n");
    bridge.push_str("int64_t miva_toml_object_len(int64_t v) { return mvp_toml_object_len((void*)(intptr_t)v); }\n");
    bridge.push_str("void* miva_toml_object_key(int64_t v, int64_t i) { auto r = mvp_toml_object_key((void*)(intptr_t)v, i); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_toml_object_get(int64_t v, int64_t i) { return (int64_t)(intptr_t)mvp_toml_object_get((void*)(intptr_t)v, i); }\n");
    bridge.push_str("int64_t miva_toml_object_find(int64_t v, void* key) { auto& k = *(std::string*)key; return (int64_t)(intptr_t)mvp_toml_object_find((void*)(intptr_t)v, k); }\n");
    bridge.push_str("void miva_toml_free(int64_t v) { mvp_toml_free((void*)(intptr_t)v); }\n");
    bridge.push_str("void* miva_toml_stringify(int64_t v) { auto r = mvp_toml_stringify((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_yaml_parse(void* s) { auto& str = *(std::string*)s; return (int64_t)(intptr_t)mvp_yaml_parse(str); }\n");
    bridge.push_str("int64_t miva_yaml_kind(int64_t v) { return mvp_yaml_kind((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_yaml_bool(int64_t v) { return mvp_yaml_bool((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_yaml_number(int64_t v) { double d = mvp_yaml_number((void*)(intptr_t)v); int64_t r; memcpy(&r, &d, 8); return r; }\n");
    bridge.push_str("void* miva_yaml_string(int64_t v) { auto r = mvp_yaml_string((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_yaml_array_len(int64_t v) { return mvp_yaml_array_len((void*)(intptr_t)v); }\n");
    bridge.push_str("int64_t miva_yaml_array_get(int64_t v, int64_t i) { return (int64_t)(intptr_t)mvp_yaml_array_get((void*)(intptr_t)v, i); }\n");
    bridge.push_str("int64_t miva_yaml_object_len(int64_t v) { return mvp_yaml_object_len((void*)(intptr_t)v); }\n");
    bridge.push_str("void* miva_yaml_object_key(int64_t v, int64_t i) { auto r = mvp_yaml_object_key((void*)(intptr_t)v, i); return new std::string(std::move(r)); }\n");
    bridge.push_str("int64_t miva_yaml_object_get(int64_t v, int64_t i) { return (int64_t)(intptr_t)mvp_yaml_object_get((void*)(intptr_t)v, i); }\n");
    bridge.push_str("int64_t miva_yaml_object_find(int64_t v, void* key) { auto& k = *(std::string*)key; return (int64_t)(intptr_t)mvp_yaml_object_find((void*)(intptr_t)v, k); }\n");
    bridge.push_str("void miva_yaml_free(int64_t v) { mvp_yaml_free((void*)(intptr_t)v); }\n");
    bridge.push_str("void* miva_yaml_stringify(int64_t v) { auto r = mvp_yaml_stringify((void*)(intptr_t)v); return new std::string(std::move(r)); }\n");
    bridge.push_str("void miva_ptr_set_i64(void* p, int64_t v) { mvp_builtin_ptrset((mvp_builtin_int*)p, v); }\n");
    bridge.push_str("void miva_ptr_set_double(void* p, double v) { mvp_builtin_ptrset((mvp_builtin_float*)p, v); }\n");
    bridge.push_str("void miva_ptr_set_i8(void* p, int8_t v) { mvp_builtin_ptrset((mvp_builtin_byte*)p, v); }\n");
    bridge.push_str("void miva_ptr_set_ptr(void* p, void* v) { mvp_builtin_ptrset((mvp_builtin_ptrany*)p, v); }\n");
    bridge.push_str("struct mvp_async_task {\n  std::mutex mutex;\n  std::condition_variable cv;\n  bool done = false;\n  int64_t result = 0;\n  std::thread thread;\n};\n");
    bridge.push_str("int64_t miva_async_spawn(int64_t (*fn)(int64_t), int64_t arg_struct_ptr) {\n  auto* task = new mvp_async_task();\n  task->thread = std::thread([task, fn, arg_struct_ptr]() {\n    int64_t r = fn(arg_struct_ptr);\n    free((void*)(intptr_t)arg_struct_ptr);\n    {\n      std::lock_guard<std::mutex> lk(task->mutex);\n      task->result = r;\n      task->done = true;\n    }\n    task->cv.notify_one();\n  });\n  return (int64_t)(intptr_t)task;\n}\n");
    bridge.push_str("int64_t miva_async_await(int64_t handle) {\n  auto* task = (mvp_async_task*)(intptr_t)handle;\n  {\n    std::unique_lock<std::mutex> lk(task->mutex);\n    task->cv.wait(lk, [&] { return task->done; });\n  }\n  int64_t r = task->result;\n  task->thread.join();\n  delete task;\n  return r;\n}\n");
    bridge.push_str("void* miva_mutex_new() { return mvp_mutex_new(); }\n");
    bridge.push_str("void miva_mutex_lock(int64_t h) { mvp_mutex_lock((void*)(intptr_t)h); }\n");
    bridge.push_str("void miva_mutex_unlock(int64_t h) { mvp_mutex_unlock((void*)(intptr_t)h); }\n");
    bridge.push_str("void miva_mutex_free(int64_t h) { mvp_mutex_free((void*)(intptr_t)h); }\n");
    bridge.push_str("}\n");
    bridge
}

