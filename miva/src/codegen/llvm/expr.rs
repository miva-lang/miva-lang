use super::*;

pub(crate) fn gen_expr(expr: &Expr, ctx: &mut LlvmCtx, body: &mut String) -> String {
    match expr {
        Expr::EInt { value, .. } => format!("{}", value),
        Expr::EBool { value, .. } => if *value { "1" } else { "0" }.to_string(),
        Expr::EString { value, .. } => {
            let resolved = crate::codegen::resolve_c_escapes(value);
            let id = STR_CONST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let const_name = format!(".str.{}", id);
            let mut llvm_escaped = String::with_capacity(resolved.len());
            for b in resolved.bytes() {
                match b {
                    b'\\' => llvm_escaped.push_str("\\\\"),
                    b'"' => llvm_escaped.push_str("\\22"),
                    0x0A => llvm_escaped.push_str("\\0A"),
                    0x0D => llvm_escaped.push_str("\\0D"),
                    0x09 => llvm_escaped.push_str("\\09"),
                    0x00 => llvm_escaped.push_str("\\00"),
                    0x20..=0x7E => llvm_escaped.push(b as char),
                    _ => llvm_escaped.push_str(&format!("\\{:02X}", b)),
                }
            }
            let len = resolved.len() + 1;
            ctx.string_constants.push_str(&format!(
                "@{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"\n",
                const_name, len, llvm_escaped
            ));
            let ptr_tmp = ctx.gen_tmp("sp");
            body.push_str(&format!(
                "{}{} = getelementptr [{} x i8], ptr @{}, i64 0, i64 0\n",
                ctx.indent_str(),
                ptr_tmp,
                len,
                const_name
            ));
            let call_tmp = ctx.gen_tmp("sc");
            body.push_str(&format!(
                "{}{} = call ptr @miva_string_from_str(ptr {})\n",
                ctx.indent_str(),
                call_tmp,
                ptr_tmp
            ));
            let int_tmp = ctx.gen_tmp("si");
            body.push_str(&format!(
                "{}{} = ptrtoint ptr {} to i64\n",
                ctx.indent_str(),
                int_tmp,
                call_tmp
            ));
            int_tmp
        }
        Expr::EFloat { value, .. } => {
            let tmp = ctx.gen_tmp("ftmp");
            body.push_str(&format!(
                "{}{} = fadd double 0.0, {}\n",
                ctx.indent_str(),
                tmp,
                value
            ));
            tmp
        }
        Expr::EChar { value, .. } => {
            let c = value.as_bytes().first().copied().unwrap_or(0) as i64;
            format!("{}", c)
        }
        Expr::EBinOp {
            op, left, right, ..
        } => {
            let l = gen_expr(left, ctx, body);
            let r = gen_expr(right, ctx, body);
            match op {
                BinOp::Add => {
                    if is_string_expr(left) || is_string_expr(right) {
                        let sp_l = ctx.gen_tmp("sp");
                        let sp_r = ctx.gen_tmp("sp");
                        body.push_str(&format!(
                            "{}{} = inttoptr i64 {} to ptr\n",
                            ctx.indent_str(),
                            sp_l,
                            l
                        ));
                        body.push_str(&format!(
                            "{}{} = inttoptr i64 {} to ptr\n",
                            ctx.indent_str(),
                            sp_r,
                            r
                        ));
                        let call_tmp = ctx.gen_tmp("call");
                        body.push_str(&format!(
                            "{}{} = call ptr @miva_string_concat(ptr {}, ptr {})\n",
                            ctx.indent_str(),
                            call_tmp,
                            sp_l,
                            sp_r
                        ));
                        let int_tmp = ctx.gen_tmp("cr");
                        body.push_str(&format!(
                            "{}{} = ptrtoint ptr {} to i64\n",
                            ctx.indent_str(),
                            int_tmp,
                            call_tmp
                        ));
                        int_tmp
                    } else {
                        let tmp = ctx.gen_tmp("add");
                        body.push_str(&format!(
                            "{}{} = add i64 {}, {}\n",
                            ctx.indent_str(),
                            tmp,
                            l,
                            r
                        ));
                        tmp
                    }
                }
                BinOp::Sub => {
                    let tmp = ctx.gen_tmp("sub");
                    body.push_str(&format!(
                        "{}{} = sub i64 {}, {}\n",
                        ctx.indent_str(),
                        tmp,
                        l,
                        r
                    ));
                    tmp
                }
                BinOp::Mul => {
                    let tmp = ctx.gen_tmp("mul");
                    body.push_str(&format!(
                        "{}{} = mul i64 {}, {}\n",
                        ctx.indent_str(),
                        tmp,
                        l,
                        r
                    ));
                    tmp
                }
                BinOp::Div => {
                    let tmp = ctx.gen_tmp("div");
                    body.push_str(&format!(
                        "{}{} = sdiv i64 {}, {}\n",
                        ctx.indent_str(),
                        tmp,
                        l,
                        r
                    ));
                    tmp
                }
                BinOp::Eq | BinOp::Neq => {
                    let want_eq = matches!(op, BinOp::Eq);
                    if is_enum_value_expr(left.as_ref()) || is_enum_value_expr(right.as_ref()) {
                        let lt = load_enum_tag(ctx, body, &l);
                        let rt = load_enum_tag(ctx, body, &r);
                        let tmp = ctx.gen_tmp("cmp");
                        let pred = if want_eq { "eq" } else { "ne" };
                        body.push_str(&format!(
                            "{}{} = icmp {} i64 {}, {}\n",
                            ctx.indent_str(),
                            tmp,
                            pred,
                            lt,
                            rt
                        ));
                        let tmp2 = ctx.gen_tmp("cmpz");
                        body.push_str(&format!(
                            "{}{} = zext i1 {} to i64\n",
                            ctx.indent_str(),
                            tmp2,
                            tmp
                        ));
                        tmp2
                    } else if let Some(agg) = aggregate_compare_type(left, right, ctx) {
                        gen_deep_compare(&l, &r, &agg, want_eq, ctx, body)
                    } else {
                        let tmp = ctx.gen_tmp("cmp");
                        let pred = if want_eq { "eq" } else { "ne" };
                        body.push_str(&format!(
                            "{}{} = icmp {} i64 {}, {}\n",
                            ctx.indent_str(),
                            tmp,
                            pred,
                            l,
                            r
                        ));
                        let tmp2 = ctx.gen_tmp("cmpz");
                        body.push_str(&format!(
                            "{}{} = zext i1 {} to i64\n",
                            ctx.indent_str(),
                            tmp2,
                            tmp
                        ));
                        tmp2
                    }
                }
                BinOp::Lt => {
                    let tmp = ctx.gen_tmp("cmp");
                    body.push_str(&format!(
                        "{}{} = icmp slt i64 {}, {}\n",
                        ctx.indent_str(),
                        tmp,
                        l,
                        r
                    ));
                    let tmp2 = ctx.gen_tmp("cmpz");
                    body.push_str(&format!(
                        "{}{} = zext i1 {} to i64\n",
                        ctx.indent_str(),
                        tmp2,
                        tmp
                    ));
                    tmp2
                }
                BinOp::Gt => {
                    let tmp = ctx.gen_tmp("cmp");
                    body.push_str(&format!(
                        "{}{} = icmp sgt i64 {}, {}\n",
                        ctx.indent_str(),
                        tmp,
                        l,
                        r
                    ));
                    let tmp2 = ctx.gen_tmp("cmpz");
                    body.push_str(&format!(
                        "{}{} = zext i1 {} to i64\n",
                        ctx.indent_str(),
                        tmp2,
                        tmp
                    ));
                    tmp2
                }
                BinOp::Le => {
                    let tmp = ctx.gen_tmp("cmp");
                    body.push_str(&format!(
                        "{}{} = icmp sle i64 {}, {}\n",
                        ctx.indent_str(),
                        tmp,
                        l,
                        r
                    ));
                    let tmp2 = ctx.gen_tmp("cmpz");
                    body.push_str(&format!(
                        "{}{} = zext i1 {} to i64\n",
                        ctx.indent_str(),
                        tmp2,
                        tmp
                    ));
                    tmp2
                }
                BinOp::Ge => {
                    let tmp = ctx.gen_tmp("cmp");
                    body.push_str(&format!(
                        "{}{} = icmp sge i64 {}, {}\n",
                        ctx.indent_str(),
                        tmp,
                        l,
                        r
                    ));
                    let tmp2 = ctx.gen_tmp("cmpz");
                    body.push_str(&format!(
                        "{}{} = zext i1 {} to i64\n",
                        ctx.indent_str(),
                        tmp2,
                        tmp
                    ));
                    tmp2
                }
                // Short-circuit logical: LLVM has no native short-circuit icmp;
                // emit `and i1`/`or i1` over the two bool i64 operands (each
                // truncated to i1 first). vec.miva relies on these only inside
                // `if (...)` conditions, so eager evaluation is fine for now.
                BinOp::And => {
                    let tmp = ctx.gen_tmp("and");
                    body.push_str(&format!(
                        "{}{} = and i64 {}, {}\n",
                        ctx.indent_str(),
                        tmp,
                        l,
                        r
                    ));
                    tmp
                }
                BinOp::Or => {
                    let tmp = ctx.gen_tmp("or");
                    body.push_str(&format!(
                        "{}{} = or i64 {}, {}\n",
                        ctx.indent_str(),
                        tmp,
                        l,
                        r
                    ));
                    tmp
                }
            }
        }
        Expr::EIf {
            cond, then, else_, ..
        } => {
            let cond_val = gen_expr(cond, ctx, body);
            let cmp = ctx.gen_tmp("ifc");
            let label_then = ctx.gen_label("then");
            let label_else = ctx.gen_label("else");
            let label_end = ctx.gen_label("ifend");
            let var_reloads_before = ctx.var_reloads.clone();
            let var_addrs_before = ctx.var_addrs.clone();
            body.push_str(&format!(
                "{}{} = icmp ne i64 {}, 0\n",
                ctx.indent_str(),
                cmp,
                cond_val
            ));
            body.push_str(&format!(
                "{}br i1 {}, label %{}, label %{}\n",
                ctx.indent_str(),
                cmp,
                label_then,
                label_else
            ));
            body.push_str(&format!("{}:\n", label_then));
            ctx.indent += 1;
            let then_val = gen_expr(then, ctx, body);
            ctx.indent -= 1;
            let var_reloads_after_then = ctx.var_reloads.clone();
            // If the `then` branch already terminates (e.g. it `return`s), do not
            // emit a fall-through `br` and do not list it as a phi predecessor.
            let then_terminated = body_ends_in_terminator(body);
            if !then_terminated {
                body.push_str(&format!("{}br label %{}\n", ctx.indent_str(), label_end));
            }
            ctx.var_reloads = var_reloads_before.clone();
            ctx.var_addrs = var_addrs_before.clone();
            body.push_str(&format!("{}:\n", label_else));
            ctx.indent += 1;
            let else_val = if let Some(else_expr) = else_ {
                gen_expr(else_expr, ctx, body)
            } else {
                "0".to_string()
            };
            ctx.indent -= 1;
            let else_terminated = body_ends_in_terminator(body);
            // If the else branch itself ended in its own merge block (a phi at
            // a fresh ifend label), the actual control flow to `label_end` is
            // from that merge. Detect this by inspecting the body's most
            // recent block — we need to forward through the inner merge
            // rather than appending an unconditional `br` here.
            let else_pred_label = label_else.clone();
            let (else_pred_label, else_ends_in_merge) =
                detect_last_merge_label(body, label_else.clone());
            if !else_terminated {
                if let Some(merge) = else_ends_in_merge.as_ref() {
                    // Forward `br label %label_end` through the inner merge
                    // block. The merge label becomes the actual predecessor
                    // of the outer phi.
                    append_br_after_merge(body, merge, &label_end);
                } else {
                    body.push_str(&format!("{}br label %{}\n", ctx.indent_str(), label_end));
                }
            }
            // Both branches terminate: the merge block is unreachable, drop it.
            if then_terminated && else_terminated {
                return "0".to_string();
            }
            body.push_str(&format!("{}:\n", label_end));
            let phi_tmp = ctx.gen_tmp("phi");
            // Only non-terminated branches actually reach the merge block.
            let mut phi_entries = String::new();
            if !then_terminated {
                phi_entries.push_str(&format!("[ {}, %{} ], ", then_val, label_then));
            }
            if let Some(merge) = else_ends_in_merge.as_ref() {
                phi_entries.push_str(&format!("[ {}, %{} ], ", else_val, merge));
            } else if !else_terminated {
                phi_entries.push_str(&format!("[ {}, %{} ], ", else_val, label_else));
            }
            if !phi_entries.is_empty() {
                // Drop the trailing ", ".
                phi_entries.truncate(phi_entries.trim_end_matches(char::is_whitespace).len());
                phi_entries.truncate(phi_entries.trim_end_matches(',').len());
                body.push_str(&format!(
                    "{}{} = phi i64 {}\n",
                    ctx.indent_str(),
                    phi_tmp,
                    phi_entries
                ));
            }
            // Restore var_addrs to pre-if state (branch-scoped allocations don't dominate post-if code)
            ctx.var_addrs = var_addrs_before;
            // Only reload variables that existed before the if (not ones declared inside branches)
            let changed_names: Vec<String> = {
                let vr = &ctx.var_reloads;
                vr.keys()
                    .filter(|name| {
                        var_reloads_before.get(*name).is_some()
                            && ctx.var_addrs.contains_key(*name)
                            && (var_reloads_after_then.get(*name) != var_reloads_before.get(*name)
                                || vr.get(*name) != var_reloads_before.get(*name))
                    })
                    .cloned()
                    .collect()
            };
            emit_fresh_loads(ctx, body, &var_reloads_before, &changed_names);
            phi_tmp
        }
        Expr::EWhile {
            cond,
            body: while_body,
            ..
        } => {
            let label_cond = ctx.gen_label("wcond");
            let label_body = ctx.gen_label("wbody");
            let label_end = ctx.gen_label("wend");
            let var_reloads_before = ctx.var_reloads.clone();
            let var_addrs_before = ctx.var_addrs.clone();
            body.push_str(&format!("{}br label %{}\n", ctx.indent_str(), label_cond));
            body.push_str(&format!("{}:\n", label_cond));
            // Reload every variable that was live before the loop so that the
            // condition references the latest values from memory (SSA is
            // immutable, so the loop body can only communicate updates through
            // memory). Without these reloads a while-loop whose condition
            // reads a variable that the body writes will loop forever.
            for (name, addr) in &var_addrs_before {
                if var_reloads_before.contains_key(name) {
                    let reload = format!("{}.reloop.{}", name, ctx.tmp_counter);
                    ctx.tmp_counter += 1;
                    body.push_str(&format!(
                        "{}%{} = load i64, ptr %{}, align 8\n",
                        ctx.indent_str(),
                        reload,
                        addr
                    ));
                    ctx.var_reloads.insert(name.clone(), reload);
                }
            }
            let cond_val = gen_expr(cond, ctx, body);
            let cmp = ctx.gen_tmp("wc");
            body.push_str(&format!(
                "{}{} = icmp ne i64 {}, 0\n",
                ctx.indent_str(),
                cmp,
                cond_val
            ));
            body.push_str(&format!(
                "{}br i1 {}, label %{}, label %{}\n",
                ctx.indent_str(),
                cmp,
                label_body,
                label_end
            ));
            body.push_str(&format!("{}:\n", label_body));
            ctx.indent += 1;
            gen_expr(while_body, ctx, body);
            ctx.indent -= 1;
            body.push_str(&format!("{}br label %{}\n", ctx.indent_str(), label_cond));
            body.push_str(&format!("{}:\n", label_end));
            // Restore var_addrs to pre-loop state
            ctx.var_addrs = var_addrs_before;
            let changed_names: Vec<String> = {
                let vr = &ctx.var_reloads;
                vr.keys()
                    .filter(|name| {
                        var_reloads_before.get(*name).is_some()
                            && ctx.var_addrs.contains_key(*name)
                            && var_reloads_before.get(*name) != vr.get(*name)
                    })
                    .cloned()
                    .collect()
            };
            emit_fresh_loads(ctx, body, &var_reloads_before, &changed_names);
            "0".to_string()
        }
        Expr::ELoop {
            body: loop_body, ..
        } => {
            let label_body = ctx.gen_label("lbody");
            let label_end = ctx.gen_label("lend");
            let var_reloads_before = ctx.var_reloads.clone();
            let var_addrs_before = ctx.var_addrs.clone();
            body.push_str(&format!("{}br label %{}\n", ctx.indent_str(), label_body));
            body.push_str(&format!("{}:\n", label_body));
            ctx.indent += 1;
            gen_expr(loop_body, ctx, body);
            ctx.indent -= 1;
            body.push_str(&format!("{}br label %{}\n", ctx.indent_str(), label_body));
            body.push_str(&format!("{}:\n", label_end));
            // Restore var_addrs to pre-loop state
            ctx.var_addrs = var_addrs_before;
            let changed_names: Vec<String> = {
                let vr = &ctx.var_reloads;
                vr.keys()
                    .filter(|name| {
                        var_reloads_before.get(*name).is_some()
                            && ctx.var_addrs.contains_key(*name)
                            && var_reloads_before.get(*name) != vr.get(*name)
                    })
                    .cloned()
                    .collect()
            };
            emit_fresh_loads(ctx, body, &var_reloads_before, &changed_names);
            "0".to_string()
        }
        Expr::EFor {
            var,
            range,
            body: for_body,
            ..
        } => {
            // `range(n)` loops n times with the loop var as the counter — the
            // range builtin returns void in LLVM, so its argument is used
            // directly as the loop bound. Any other range value is an array
            // (`{len, e0, e1, ...}` heap block, see EArrayLit): loop over the
            // indices and load each element into the loop var.
            let (bound, arr_ptr) = match range.as_ref() {
                Expr::ECall { name, args, .. } if name == "range" && args.len() == 1 => {
                    (gen_expr(&args[0], ctx, body), None)
                }
                _ => {
                    let arr = gen_expr(range, ctx, body);
                    let ptr = ctx.gen_tmp("farr");
                    body.push_str(&format!(
                        "{}{} = inttoptr i64 {} to ptr\n",
                        ctx.indent_str(),
                        ptr,
                        arr
                    ));
                    let len = ctx.gen_tmp("flen");
                    body.push_str(&format!(
                        "{}{} = load i64, ptr {}\n",
                        ctx.indent_str(),
                        len,
                        ptr
                    ));
                    (len, Some(ptr))
                }
            };
            let label_cond = ctx.gen_label("fcond");
            let label_body = ctx.gen_label("fbody");
            let label_end = ctx.gen_label("fend");
            let var_reloads_before = ctx.var_reloads.clone();
            let var_addrs_before = ctx.var_addrs.clone();
            // Declare the loop variable (creates an alloca address)
            let (addr, _reload) = ctx.declare_var(var);
            body.push_str(&format!("  %{} = alloca i64, align 8\n", addr));
            // The counter is the loop var itself for range(n); arrays keep the
            // index in a separate slot so the loop var can hold the element.
            let counter_addr = if arr_ptr.is_some() {
                let c = format!("fidx.{}", ctx.tmp_counter);
                ctx.tmp_counter += 1;
                body.push_str(&format!("  %{} = alloca i64, align 8\n", c));
                c
            } else {
                addr.clone()
            };
            // Initialize loop counter to 0
            body.push_str(&format!("  store i64 0, ptr %{}, align 8\n", counter_addr));
            body.push_str(&format!("{}br label %{}\n", ctx.indent_str(), label_cond));
            body.push_str(&format!("{}:\n", label_cond));
            // Reload loop counter
            let counter_name = format!("{}.fv.{}", var, ctx.tmp_counter);
            ctx.tmp_counter += 1;
            body.push_str(&format!(
                "  %{} = load i64, ptr %{}, align 8\n",
                counter_name, counter_addr
            ));
            let cmp = ctx.gen_tmp("fc");
            body.push_str(&format!(
                "{}{} = icmp slt i64 %{}, {}\n",
                ctx.indent_str(),
                cmp,
                counter_name,
                bound
            ));
            body.push_str(&format!(
                "{}br i1 {}, label %{}, label %{}\n",
                ctx.indent_str(),
                cmp,
                label_body,
                label_end
            ));
            body.push_str(&format!("{}:\n", label_body));
            ctx.indent += 1;
            let reload_name = if let Some(ptr) = &arr_ptr {
                // Load the element at counter+1 into the loop var.
                let off = ctx.gen_tmp("feo");
                body.push_str(&format!(
                    "{}{} = add i64 %{}, 1\n",
                    ctx.indent_str(),
                    off,
                    counter_name
                ));
                let gep = ctx.gen_tmp("feg");
                body.push_str(&format!(
                    "{}{} = getelementptr i64, ptr {}, i64 {}\n",
                    ctx.indent_str(),
                    gep,
                    ptr,
                    off
                ));
                let elem = ctx.gen_tmp("fel");
                body.push_str(&format!(
                    "{}{} = load i64, ptr {}\n",
                    ctx.indent_str(),
                    elem,
                    gep
                ));
                body.push_str(&format!(
                    "{}store i64 {}, ptr %{}, align 8\n",
                    ctx.indent_str(),
                    elem,
                    addr
                ));
                let r = format!("{}.fv.{}", var, ctx.tmp_counter);
                ctx.tmp_counter += 1;
                body.push_str(&format!(
                    "{}%{} = load i64, ptr %{}, align 8\n",
                    ctx.indent_str(),
                    r,
                    addr
                ));
                r
            } else {
                counter_name.clone()
            };
            ctx.var_reloads.insert(var.clone(), reload_name);
            gen_expr(for_body, ctx, body);
            ctx.indent -= 1;
            // Increment and store loop counter back
            let next_name = format!("{}.fn.{}", var, ctx.tmp_counter);
            ctx.tmp_counter += 1;
            body.push_str(&format!(
                "  %{} = add i64 %{}, 1\n",
                next_name, counter_name
            ));
            body.push_str(&format!(
                "  store i64 %{}, ptr %{}, align 8\n",
                next_name, counter_addr
            ));
            body.push_str(&format!("{}br label %{}\n", ctx.indent_str(), label_cond));
            body.push_str(&format!("{}:\n", label_end));
            // Loop variable is out of scope after the loop
            ctx.var_reloads.remove(var);
            // Restore var_addrs to pre-loop state (loop-scoped allocations don't dominate post-loop code)
            ctx.var_addrs = var_addrs_before;
            // Only reload variables that existed before the loop (not ones declared inside)
            let changed_names: Vec<String> = {
                let vr = &ctx.var_reloads;
                vr.keys()
                    .filter(|name| {
                        var_reloads_before.get(*name).is_some()
                            && ctx.var_addrs.contains_key(*name)
                            && var_reloads_before.get(*name) != vr.get(*name)
                    })
                    .cloned()
                    .collect()
            };
            emit_fresh_loads(ctx, body, &var_reloads_before, &changed_names);
            "0".to_string()
        }
        Expr::EVar { name, .. } | Expr::EMove { name, .. } | Expr::EClone { name, .. } => {
            ctx.get_var_reload(name)
        }
        Expr::EVoid { .. } => "0".to_string(),
        Expr::ECall {
            name,
            type_args,
            args,
            ..
        } => gen_call(name, args, type_args, ctx, body),
        Expr::EMethodCall {
            method,
            type_args,
            args,
            ..
        } => gen_call(method, args, type_args, ctx, body),
        Expr::ECast { expr, to, .. } => {
            let val = gen_expr(expr, ctx, body);
            match to {
                Typ::TFloat64 => {
                    let tmp = ctx.gen_tmp("cast");
                    body.push_str(&format!(
                        "{}{} = sitofp i64 {} to double\n",
                        ctx.indent_str(),
                        tmp,
                        val
                    ));
                    tmp
                }
                Typ::TFloat32 => {
                    let tmp = ctx.gen_tmp("cast");
                    body.push_str(&format!(
                        "{}{} = sitofp i64 {} to float\n",
                        ctx.indent_str(),
                        tmp,
                        val
                    ));
                    tmp
                }
                _ => val,
            }
        }
        Expr::EBlock { stmts, result, .. } => {
            // Names `let`-declared directly in this block are scoped to it:
            // snapshot their outer bindings and restore them afterwards so an
            // inner shadowing `let` (e.g. from macro expansion) cannot clobber
            // an outer variable of the same name.
            let mut declared: Vec<String> = stmts
                .iter()
                .filter_map(|s| match s {
                    Stmt::SLet { name, .. } | Stmt::SLetTyped { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect();
            for s in stmts {
                if let Stmt::SLetTuple { patterns, .. } = s {
                    for n in patterns {
                        declared.push(n.clone());
                    }
                }
            }
            let saved: Vec<(String, Option<String>, Option<String>, Option<Typ>)> = declared
                .iter()
                .map(|n| {
                    (
                        n.clone(),
                        ctx.var_addrs.get(n).cloned(),
                        ctx.var_reloads.get(n).cloned(),
                        ctx.var_types.get(n).cloned(),
                    )
                })
                .collect();
            for stmt in stmts {
                gen_stmt(stmt, ctx, body);
            }
            let out = match result {
                Some(r) => gen_expr(r, ctx, body),
                None => "0".to_string(),
            };
            for (name, addr, reload, typ) in saved {
                match addr {
                    Some(a) => {
                        ctx.var_addrs.insert(name.clone(), a);
                    }
                    None => {
                        ctx.var_addrs.remove(&name);
                    }
                }
                match reload {
                    Some(r) => {
                        ctx.var_reloads.insert(name.clone(), r);
                    }
                    None => {
                        ctx.var_reloads.remove(&name);
                    }
                }
                match typ {
                    Some(t) => {
                        ctx.var_types.insert(name.clone(), t);
                    }
                    None => {
                        ctx.var_types.remove(&name);
                    }
                }
            }
            out
        }
        Expr::EStructLit {
            name: struct_name,
            fields,
            ..
        } => {
            let tmp = ctx.gen_tmp("st");
            let size = fields.len() * 8;
            body.push_str(&format!(
                "{}{} = call ptr @miva_alloc(i64 {})\n",
                ctx.indent_str(),
                tmp,
                size
            ));
            for (i, f) in fields.iter().enumerate() {
                let fv = gen_expr(&f.value, ctx, body);
                let gep = ctx.gen_tmp("sf");
                body.push_str(&format!(
                    "{}{} = getelementptr i8, ptr {}, i64 {}\n",
                    ctx.indent_str(),
                    gep,
                    tmp,
                    i * 8
                ));
                let gep_typed = ctx.gen_tmp("sf");
                body.push_str(&format!(
                    "{}{} = bitcast ptr {} to ptr\n",
                    ctx.indent_str(),
                    gep_typed,
                    gep
                ));
                body.push_str(&format!(
                    "{}store i64 {}, ptr {}\n",
                    ctx.indent_str(),
                    fv,
                    gep_typed
                ));
            }
            let int_tmp = ctx.gen_tmp("stint");
            body.push_str(&format!(
                "{}{} = ptrtoint ptr {} to i64\n",
                ctx.indent_str(),
                int_tmp,
                tmp
            ));
            int_tmp
        }
        Expr::EFieldAccess {
            expr: fexpr, field, ..
        } => {
            if field.chars().all(|c| c.is_ascii_digit()) {
                // Numeric access indexes a tuple (offset 0) or, in a choose/when
                // destructure, an enum payload (offset 1, after the tag). Decide
                // from the base expression's inferred type; fall back to tuple.
                let is_enum_payload = infer_expr_type(fexpr, ctx).map_or(
                    false,
                    |t| matches!(&t, Typ::TStruct { name, .. } if ctx.enum_defs.contains_key(name)),
                );
                let val = gen_expr(fexpr, ctx, body);
                let ptr_val = ctx.gen_tmp("fa_ptr");
                body.push_str(&format!(
                    "{}{} = inttoptr i64 {} to ptr\n",
                    ctx.indent_str(),
                    ptr_val,
                    val
                ));
                let idx: i64 =
                    field.parse::<i64>().unwrap_or(0) + if is_enum_payload { 1 } else { 0 };
                let gep = ctx.gen_tmp("fa");
                body.push_str(&format!(
                    "{}{} = getelementptr i64, ptr {}, i64 {}\n",
                    ctx.indent_str(),
                    gep,
                    ptr_val,
                    idx
                ));
                let load = ctx.gen_tmp("fal");
                body.push_str(&format!(
                    "{}{} = load i64, ptr {}\n",
                    ctx.indent_str(),
                    load,
                    gep
                ));
                if let Some(ft) = infer_field_type(fexpr, field, ctx) {
                    if typ_is_string(&ft) {
                        ctx.string_regs.insert(load.clone());
                    }
                }
                return load;
            } else if let Expr::EVar {
                name: enum_name, ..
            } = fexpr.as_ref()
            {
                if enum_name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    let ctor_name = format!("{}_{}_unit", enum_name, field);
                    if !ctx.enum_defs.contains_key(enum_name) {
                        if let Ok(mut guard) = EXTERN_DECLS.lock() {
                            let decl = format!("declare i64 @{}()", ctor_name);
                            guard.get_or_insert_with(HashSet::new).insert(decl);
                        }
                    }
                    let tmp = ctx.gen_tmp("disc");
                    body.push_str(&format!(
                        "{}{} = call i64 @{}_{}_unit()\n",
                        ctx.indent_str(),
                        tmp,
                        enum_name,
                        field
                    ));
                    return tmp;
                }
                // Base is a variable but not an uppercase enum discriminant.
                let val = gen_expr(fexpr, ctx, body);
                // Compute field index based on the base expression's type if it's a known variable.
                let field_idx = {
                    let mut idx = None;
                    if let Expr::EVar {
                        name: ref vname, ..
                    } = **fexpr
                    {
                        if let Some(typ) = ctx.var_types.get(vname) {
                            match typ {
                                Typ::TStruct {
                                    name: struct_name, ..
                                } => {
                                    if let Some(struct_map) = ctx.struct_field_map.get(struct_name)
                                    {
                                        idx = struct_map.get(field).copied();
                                    }
                                }
                                Typ::TShape {
                                    name: shape_name, ..
                                } => {
                                    if let Some(struct_map) = ctx.struct_field_map.get(shape_name) {
                                        idx = struct_map.get(field).copied();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    if idx.is_none() {
                        idx = ctx.field_idx.get(field).copied();
                    }
                    idx.unwrap_or(0)
                };
                let ptr_val = ctx.gen_tmp("fa_ptr");
                body.push_str(&format!(
                    "{}{} = inttoptr i64 {} to ptr\n",
                    ctx.indent_str(),
                    ptr_val,
                    val
                ));
                let gep = ctx.gen_tmp("fa");
                body.push_str(&format!(
                    "{}{} = getelementptr i64, ptr {}, i64 {}\n",
                    ctx.indent_str(),
                    gep,
                    ptr_val,
                    field_idx
                ));
                let load = ctx.gen_tmp("fal");
                body.push_str(&format!(
                    "{}{} = load i64, ptr {}\n",
                    ctx.indent_str(),
                    load,
                    gep
                ));
                if field_access_is_string(fexpr.as_ref(), field, ctx) {
                    ctx.string_regs.insert(load.clone());
                }
                load
            } else {
                // Base expression is not a simple variable; use fallback field index.
                let val = gen_expr(fexpr, ctx, body);
                let ptr_val = ctx.gen_tmp("fa_ptr");
                body.push_str(&format!(
                    "{}{} = inttoptr i64 {} to ptr\n",
                    ctx.indent_str(),
                    ptr_val,
                    val
                ));
                let field_idx = ctx.field_idx.get(field).copied().unwrap_or(0);
                let gep = ctx.gen_tmp("fa");
                body.push_str(&format!(
                    "{}{} = getelementptr i64, ptr {}, i64 {}\n",
                    ctx.indent_str(),
                    gep,
                    ptr_val,
                    field_idx
                ));
                let load = ctx.gen_tmp("fal");
                body.push_str(&format!(
                    "{}{} = load i64, ptr {}\n",
                    ctx.indent_str(),
                    load,
                    gep
                ));
                load
            }
        }
        Expr::EChoose {
            loc,
            var,
            cases,
            otherwise,
            ..
        } => {
            // Translate `choose` into a nested if/else chain, reusing the
            // existing EIf codegen (phi merging, branch-terminator handling).
            // Each case becomes `if (var == when) then else <rest>`.
            // An `EEnumPattern` destructure (`when (Enum.Variant(x, y))`) is
            // desugared: the `when` becomes the variant discriminant (a tag-only
            // value so the existing enum tag-equality logic applies), and the
            // `then` branch is wrapped to bind each payload field (`var.0`,
            // `var.1`, ...) to its pattern name via `let`.
            let loc = loc.clone();
            struct Case {
                when: Expr,
                guard: Option<Expr>,
                bindings: Vec<Stmt>,
                body: Expr,
            }
            let transformed: Vec<Case> = cases
                .iter()
                .map(|case| {
                    let guard = case.guard.as_ref().map(|g| g.as_ref().clone());
                    match case.when.as_ref() {
                        Expr::EEnumPattern {
                            enum_name,
                            variant,
                            bindings,
                            ..
                        } => {
                            let when = Expr::EFieldAccess {
                                loc: loc.clone(),
                                expr: Box::new(Expr::EVar {
                                    loc: loc.clone(),
                                    name: enum_name.clone(),
                                }),
                                field: variant.clone(),
                            };
                            let lets: Vec<Stmt> = bindings
                                .iter()
                                .enumerate()
                                .map(|(i, b)| Stmt::SLet {
                                    loc: loc.clone(),
                                    mutable: false,
                                    name: b.clone(),
                                    expr: Box::new(Expr::EFieldAccess {
                                        loc: loc.clone(),
                                        expr: var.clone(),
                                        field: i.to_string(),
                                    }),
                                })
                                .collect();
                            let body = match case.then.as_ref() {
                                Expr::EBlock {
                                    loc: bl,
                                    stmts,
                                    result,
                                    ..
                                } => Expr::EBlock {
                                    loc: bl.clone(),
                                    stmts: stmts.iter().cloned().collect(),
                                    result: result.clone(),
                                },
                                other => Expr::EBlock {
                                    loc: loc.clone(),
                                    stmts: vec![],
                                    result: Some(Box::new(other.clone())),
                                },
                            };
                            Case {
                                when,
                                guard,
                                bindings: lets,
                                body,
                            }
                        }
                        _ => Case {
                            when: case.when.as_ref().clone(),
                            guard,
                            bindings: vec![],
                            body: case.then.as_ref().clone(),
                        },
                    }
                })
                .collect();
            let mut chain: Expr = match otherwise {
                Some(e) => *e.clone(),
                None => Expr::EInt {
                    loc: loc.clone(),
                    value: 0,
                },
            };
            for c in transformed.iter().rev() {
                // Bindings are emitted *before* the guard so the guard
                // condition can reference the destructured payload names
                // (it must see `n` before evaluating `n > 0`).
                let mut inner_block: Vec<Stmt> = c.bindings.clone();
                let guard_wrapped = match &c.guard {
                    Some(g) => {
                        inner_block.push(Stmt::SExpr {
                            loc: loc.clone(),
                            expr: Box::new(Expr::EIf {
                                loc: loc.clone(),
                                cond: Box::new(g.clone()),
                                then: Box::new(c.body.clone()),
                                else_: Some(Box::new(chain.clone())),
                            }),
                        });
                        Expr::EBlock {
                            loc: loc.clone(),
                            stmts: inner_block,
                            result: None,
                        }
                    }
                    None => {
                        inner_block.push(Stmt::SExpr {
                            loc: loc.clone(),
                            expr: Box::new(c.body.clone()),
                        });
                        Expr::EBlock {
                            loc: loc.clone(),
                            stmts: inner_block,
                            result: None,
                        }
                    }
                };
                chain = Expr::EIf {
                    loc: loc.clone(),
                    cond: Box::new(Expr::EBinOp {
                        loc: loc.clone(),
                        op: BinOp::Eq,
                        left: var.clone(),
                        right: Box::new(c.when.clone()),
                    }),
                    then: Box::new(guard_wrapped),
                    else_: Some(Box::new(chain)),
                };
            }
            gen_expr(&chain, ctx, body)
        }
        Expr::EArrayLit { values, .. } => {
            // Arrays are `{len, e0, e1, ...}` heap blocks of i64 slots,
            // mirroring the enum/struct representation in this backend.
            let vals: Vec<String> = values.iter().map(|v| gen_expr(v, ctx, body)).collect();
            let tmp = ctx.gen_tmp("arr");
            let size = (values.len() + 1) * 8;
            body.push_str(&format!(
                "{}{} = call ptr @miva_alloc(i64 {})\n",
                ctx.indent_str(),
                tmp,
                size
            ));
            body.push_str(&format!(
                "{}store i64 {}, ptr {}\n",
                ctx.indent_str(),
                values.len(),
                tmp
            ));
            for (i, v) in vals.iter().enumerate() {
                let gep = ctx.gen_tmp("ae");
                body.push_str(&format!(
                    "{}{} = getelementptr i64, ptr {}, i64 {}\n",
                    ctx.indent_str(),
                    gep,
                    tmp,
                    i + 1
                ));
                body.push_str(&format!(
                    "{}store i64 {}, ptr {}\n",
                    ctx.indent_str(),
                    v,
                    gep
                ));
            }
            let int_tmp = ctx.gen_tmp("arri");
            body.push_str(&format!(
                "{}{} = ptrtoint ptr {} to i64\n",
                ctx.indent_str(),
                int_tmp,
                tmp
            ));
            int_tmp
        }
        Expr::ETupleLit { values, .. } => {
            let vals: Vec<String> = values.iter().map(|v| gen_expr(v, ctx, body)).collect();
            let tmp = ctx.gen_tmp("tp");
            let size = values.len() * 8;
            body.push_str(&format!(
                "{}{} = call ptr @miva_alloc(i64 {})\n",
                ctx.indent_str(),
                tmp,
                size
            ));
            for (i, v) in vals.iter().enumerate() {
                let gep = ctx.gen_tmp("te");
                body.push_str(&format!(
                    "{}{} = getelementptr i64, ptr {}, i64 {}\n",
                    ctx.indent_str(),
                    gep,
                    tmp,
                    i
                ));
                body.push_str(&format!(
                    "{}store i64 {}, ptr {}\n",
                    ctx.indent_str(),
                    v,
                    gep
                ));
            }
            let int_tmp = ctx.gen_tmp("tpi");
            body.push_str(&format!(
                "{}{} = ptrtoint ptr {} to i64\n",
                ctx.indent_str(),
                int_tmp,
                tmp
            ));
            int_tmp
        }
        Expr::EAddr { expr: aexpr, .. } => gen_expr(aexpr, ctx, body),
        Expr::EDeref { expr: dexpr, .. } => {
            // Pointer values are modelled as i64 addresses in the LLVM backend,
            // so `gen_expr(dexpr)` yields an i64 holding the address. Convert it
            // back to a real pointer before dereferencing.
            let val = gen_expr(dexpr, ctx, body);
            let ptr_tmp = ctx.gen_tmp("deref_ptr");
            body.push_str(&format!(
                "{}{} = inttoptr i64 {} to ptr\n",
                ctx.indent_str(),
                ptr_tmp,
                val
            ));
            let tmp = ctx.gen_tmp("deref");
            body.push_str(&format!(
                "{}{} = load i64, ptr {}\n",
                ctx.indent_str(),
                tmp,
                ptr_tmp
            ));
            tmp
        }
        Expr::EMacro { .. } | Expr::EMacroVar { .. } => "0".to_string(),
        Expr::EEnumPattern { .. } => {
            unreachable!("EEnumPattern is handled inline in the EChoose arm")
        }
        Expr::ELambda {
            params,
            ret,
            captures,
            body: lambda_body,
            ..
        } => {
            // Lower a lambda to a closure value: a pointer to a heap struct
            // `{ i64 env, i64 fn }` where `env` points to the captured values
            // and `fn` is the thunk function pointer.
            let thunk_name = gen_closure_thunk(
                captures,
                params,
                ret,
                lambda_body,
                &ctx.struct_field_map,
                &ctx.struct_field_types,
                &ctx.func_sigs,
                &ctx.enum_defs,
            );

            // Allocate the capture environment struct and store each capture.
            let env_size = (captures.len() as i64) * 8;
            let env_ptr = ctx.gen_tmp("cenv");
            body.push_str(&format!(
                "  {} = call ptr @miva_alloc(i64 {})\n",
                env_ptr, env_size
            ));
            for (i, (cap_name, _)) in captures.iter().enumerate() {
                let cap_val = gen_expr(
                    &Expr::EVar {
                        name: cap_name.clone(),
                        loc: Loc { line: 0, col: 0 },
                    },
                    ctx,
                    body,
                );
                let gep = ctx.gen_tmp("cgep");
                body.push_str(&format!(
                    "  {} = getelementptr i64, ptr {}, i64 {}\n",
                    gep, env_ptr, i
                ));
                body.push_str(&format!("  store i64 {}, ptr {}\n", cap_val, gep));
            }

            // Allocate the closure struct and store (env, fn).
            let clo_ptr = ctx.gen_tmp("clo");
            body.push_str(&format!("  {} = call ptr @miva_alloc(i64 16)\n", clo_ptr));
            let fn_int = ctx.gen_tmp("fnint");
            body.push_str(&format!(
                "  {} = ptrtoint i64 (...) * @{} to i64\n",
                fn_int, thunk_name
            ));
            let env_int = ctx.gen_tmp("envint");
            body.push_str(&format!(
                "  {} = ptrtoint ptr {} to i64\n",
                env_int, env_ptr
            ));
            let clo_gep0 = ctx.gen_tmp("clogep0");
            body.push_str(&format!(
                "  {} = getelementptr i64, ptr {}, i64 0\n",
                clo_gep0, clo_ptr
            ));
            body.push_str(&format!("  store i64 {}, ptr {}\n", env_int, clo_gep0));
            let clo_gep1 = ctx.gen_tmp("clogep1");
            body.push_str(&format!(
                "  {} = getelementptr i64, ptr {}, i64 1\n",
                clo_gep1, clo_ptr
            ));
            body.push_str(&format!("  store i64 {}, ptr {}\n", fn_int, clo_gep1));

            let clo_int = ctx.gen_tmp("clo_int");
            body.push_str(&format!(
                "  {} = ptrtoint ptr {} to i64\n",
                clo_int, clo_ptr
            ));
            clo_int
        }
    }
}

/// Deep `==`/`!=` for tuple/struct values held in already-materialized
/// registers `l`/`r`: compares each field (recursively) and ANDs the results.
/// `want_eq` selects equality (true) or inequality (false).
pub(crate) fn gen_deep_compare(
    l: &str,
    r: &str,
    typ: &Typ,
    want_eq: bool,
    ctx: &mut LlvmCtx,
    body: &mut String,
) -> String {
    let result = match typ {
        Typ::TTuple { elems } => {
            let mut results = Vec::with_capacity(elems.len());
            for (i, et) in elems.iter().enumerate() {
                let lf = load_field_reg(l, i as i64, ctx, body);
                let rf = load_field_reg(r, i as i64, ctx, body);
                results.push(gen_deep_compare(&lf, &rf, et, true, ctx, body));
            }
            let combined = combine_and(results, ctx, body);
            if want_eq {
                combined
            } else {
                not_reg(&combined, ctx, body)
            }
        }
        Typ::TStruct { name, .. } if ctx.enum_defs.contains_key(name) => {
            let lt = load_enum_tag(ctx, body, l);
            let rt = load_enum_tag(ctx, body, r);
            let tmp = ctx.gen_tmp("cmp");
            body.push_str(&format!(
                "{}{} = icmp eq i64 {}, {}\n",
                ctx.indent_str(),
                tmp,
                lt,
                rt
            ));
            let tmp2 = ctx.gen_tmp("cmpz");
            body.push_str(&format!(
                "{}{} = zext i1 {} to i64\n",
                ctx.indent_str(),
                tmp2,
                tmp
            ));
            tmp2
        }
        Typ::TStruct { name, .. } => {
            if let Some(fmap) = ctx.struct_field_types.get(name) {
                let mut fields: Vec<(String, Typ)> =
                    fmap.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                fields.sort_by_key(|(fname, _)| {
                    ctx.struct_field_map
                        .get(name)
                        .and_then(|m| m.get(fname))
                        .copied()
                        .unwrap_or(0)
                });
                let mut results = Vec::with_capacity(fields.len());
                for (fname, ftyp) in fields {
                    let idx = ctx
                        .struct_field_map
                        .get(name)
                        .and_then(|m| m.get(&fname))
                        .copied()
                        .unwrap_or(0) as i64;
                    let lf = load_field_reg(l, idx, ctx, body);
                    let rf = load_field_reg(r, idx, ctx, body);
                    results.push(gen_deep_compare(&lf, &rf, &ftyp, true, ctx, body));
                }
                let combined = combine_and(results, ctx, body);
                if want_eq {
                    combined
                } else {
                    not_reg(&combined, ctx, body)
                }
            } else {
                scalar_compare(l, r, want_eq, ctx, body)
            }
        }
        Typ::TString => {
            if let Ok(mut guard) = EXTERN_DECLS.lock() {
                let decl = "declare i64 @miva_string_eq(ptr, ptr)";
                guard
                    .get_or_insert_with(HashSet::new)
                    .insert(decl.to_string());
            }
            let pl = ctx.gen_tmp("seqp");
            body.push_str(&format!(
                "{}{} = inttoptr i64 {} to ptr\n",
                ctx.indent_str(),
                pl,
                l
            ));
            let pr = ctx.gen_tmp("seqp");
            body.push_str(&format!(
                "{}{} = inttoptr i64 {} to ptr\n",
                ctx.indent_str(),
                pr,
                r
            ));
            let tmp = ctx.gen_tmp("seq");
            body.push_str(&format!(
                "{}{} = call i64 @miva_string_eq(ptr {}, ptr {})\n",
                ctx.indent_str(),
                tmp,
                pl,
                pr
            ));
            if want_eq {
                tmp
            } else {
                not_reg(&tmp, ctx, body)
            }
        }
        _ => scalar_compare(l, r, want_eq, ctx, body),
    };
    result
}

fn load_field_reg(base_reg: &str, idx: i64, ctx: &mut LlvmCtx, body: &mut String) -> String {
    let ptr = ctx.gen_tmp("dfp");
    body.push_str(&format!(
        "{}{} = inttoptr i64 {} to ptr\n",
        ctx.indent_str(),
        ptr,
        base_reg
    ));
    let gep = ctx.gen_tmp("dfg");
    body.push_str(&format!(
        "{}{} = getelementptr i64, ptr {}, i64 {}\n",
        ctx.indent_str(),
        gep,
        ptr,
        idx
    ));
    let load = ctx.gen_tmp("dfl");
    body.push_str(&format!(
        "{}{} = load i64, ptr {}\n",
        ctx.indent_str(),
        load,
        gep
    ));
    load
}

fn scalar_compare(l: &str, r: &str, want_eq: bool, ctx: &mut LlvmCtx, body: &mut String) -> String {
    let tmp = ctx.gen_tmp("cmp");
    let pred = if want_eq { "eq" } else { "ne" };
    body.push_str(&format!(
        "{}{} = icmp {} i64 {}, {}\n",
        ctx.indent_str(),
        tmp,
        pred,
        l,
        r
    ));
    let tmp2 = ctx.gen_tmp("cmpz");
    body.push_str(&format!(
        "{}{} = zext i1 {} to i64\n",
        ctx.indent_str(),
        tmp2,
        tmp
    ));
    tmp2
}

fn not_reg(reg: &str, ctx: &mut LlvmCtx, body: &mut String) -> String {
    let tmp = ctx.gen_tmp("not");
    body.push_str(&format!(
        "{}{} = xor i64 {}, 1\n",
        ctx.indent_str(),
        tmp,
        reg
    ));
    tmp
}

fn combine_and(results: Vec<String>, ctx: &mut LlvmCtx, body: &mut String) -> String {
    if results.is_empty() {
        return "1".to_string();
    }
    let mut acc = results[0].clone();
    for c in results.iter().skip(1) {
        let tmp = ctx.gen_tmp("and");
        body.push_str(&format!(
            "{}{} = and i64 {}, {}\n",
            ctx.indent_str(),
            tmp,
            acc,
            c
        ));
        acc = tmp;
    }
    acc
}

/// Whether the i-th argument of a runtime function should be passed as `ptr`
/// instead of `i64`. Used to convert string handles back to pointers.
pub(crate) fn is_ptr_arg(func_name: &str, arg_idx: usize) -> bool {
    match func_name {
        n if n == "@miva_panic" => true,
        n if n.contains("miva_print") || n.contains("miva_error") => true,
        "@miva_string_concat" | "@miva_string_from_str" => true,
        "@miva_string_parse" | "@miva_string_length" => true,
        "@miva_string_make" => arg_idx == 0,
        // @miva_box_new_string takes ptr for both args (the box ptr and the string value)
        "@miva_box_new_string" => arg_idx == 0 || arg_idx == 1,
        "@miva_box_new_int"
        | "@miva_box_new_float"
        | "@miva_box_new_bool"
        | "@miva_box_new_byte" => arg_idx == 0,
        "@miva_box_deref_int"
        | "@miva_box_deref_float"
        | "@miva_box_deref_bool"
        | "@miva_box_deref_byte" => false,
        "@miva_box_deref_string" => true,
        "@miva_range" | "@miva_range_end" | "@miva_range_step" => arg_idx == 0,
        "@miva_json_parse" => arg_idx == 0,
        "@miva_json_object_find" => arg_idx == 1,
        "@miva_xml_parse" => arg_idx == 0,
        "@miva_xml_attr_find" => arg_idx == 1,
        "@miva_toml_parse" => arg_idx == 0,
        "@miva_toml_object_find" => arg_idx == 1,
        "@miva_yaml_parse" => arg_idx == 0,
        "@miva_yaml_object_find" => arg_idx == 1,
        // Pointer-manipulation builtins (all take ptr as first arg)
        "@miva_realloc" | "@miva_free" | "@miva_ptr_offset" => arg_idx == 0,
        "@miva_ptr_set_i64" | "@miva_ptr_set_double" | "@miva_ptr_set_i8" | "@miva_ptr_set_ptr" => {
            arg_idx == 0
        }
        "@miva_async_spawn" => arg_idx == 0,
        _ => false,
    }
}

pub(crate) fn ret_type(func_name: &str) -> &'static str {
    match func_name {
        n if n.contains("miva_print")
            || n.contains("miva_error")
            || n == "@miva_panic"
            || n == "@miva_abort"
            || n == "@miva_exit" =>
        {
            "void"
        }
        n if n == "@miva_string_concat"
            || n == "@miva_string_make"
            || n.starts_with("@miva_string_from_")
            || n == "@miva_alloc"
            || n == "@miva_realloc"
            || n == "@miva_json_string"
            || n == "@miva_json_object_key"
            || n == "@miva_json_stringify" =>
        {
            "ptr"
        }
        n if n == "@miva_xml_tag"
            || n == "@miva_xml_attr_name"
            || n == "@miva_xml_attr_value"
            || n == "@miva_xml_attr_find"
            || n == "@miva_xml_text"
            || n == "@miva_xml_comment"
            || n == "@miva_xml_cdata"
            || n == "@miva_xml_pi_target"
            || n == "@miva_xml_pi_data"
            || n == "@miva_xml_stringify" =>
        {
            "ptr"
        }
        n if n == "@miva_toml_string"
            || n == "@miva_toml_object_key"
            || n == "@miva_toml_stringify" =>
        {
            "ptr"
        }
        n if n == "@miva_yaml_string"
            || n == "@miva_yaml_object_key"
            || n == "@miva_yaml_stringify" =>
        {
            "ptr"
        }
        n if n == "@miva_box_deref_float" => "double",
        n if n == "@miva_box_deref_bool" || n == "@miva_box_deref_byte" => "i8",
        // Pointer-manipulation builtins
        "@miva_ptr_offset" => "ptr",
        "@miva_free"
        | "@miva_ptr_set_i64"
        | "@miva_ptr_set_double"
        | "@miva_ptr_set_i8"
        | "@miva_ptr_set_ptr" => "void",
        n if n.starts_with("@miva_") || n.starts_with("@ffi_") => "i64",
        _ => "i64",
    }
}

pub(crate) fn gen_call(
    name: &str,
    args: &[Expr],
    type_args: &[Typ],
    ctx: &mut LlvmCtx,
    body: &mut String,
) -> String {
    let lookup = name.rsplit('.').next().unwrap_or(name);
    // Calling a closure-typed variable: load the closure struct, extract the
    // environment pointer and the thunk function pointer, then call the thunk
    // indirectly with `(env, args...)`.
    if let Some(Typ::TFunc { .. }) = ctx.var_types.get(lookup) {
        let clo_int = gen_expr(
            &Expr::EVar {
                name: lookup.to_string(),
                loc: Loc { line: 0, col: 0 },
            },
            ctx,
            body,
        );
        let clo_ptr = ctx.gen_tmp("cloptr");
        body.push_str(&format!(
            "  {} = inttoptr i64 {} to ptr\n",
            clo_ptr, clo_int
        ));
        let env_gep = ctx.gen_tmp("cenvgep");
        body.push_str(&format!(
            "  {} = getelementptr i64, ptr {}, i64 0\n",
            env_gep, clo_ptr
        ));
        let env_val = ctx.gen_tmp("cenvval");
        body.push_str(&format!("  {} = load i64, ptr {}\n", env_val, env_gep));
        let fn_gep = ctx.gen_tmp("cngep");
        body.push_str(&format!(
            "  {} = getelementptr i64, ptr {}, i64 1\n",
            fn_gep, clo_ptr
        ));
        let fn_int = ctx.gen_tmp("cnfn");
        body.push_str(&format!("  {} = load i64, ptr {}\n", fn_int, fn_gep));
        let fn_ptr = ctx.gen_tmp("cnfnptr");
        body.push_str(&format!("  {} = inttoptr i64 {} to ptr\n", fn_ptr, fn_int));

        let mut arg_strs = vec![format!("i64 {}", env_val)];
        for a in args {
            let t = gen_expr(a, ctx, body);
            arg_strs.push(format!("i64 {}", t));
        }
        let tmp = ctx.gen_tmp("ccl");
        body.push_str(&format!(
            "  {} = call i64 {}({})\n",
            tmp,
            fn_ptr,
            arg_strs.join(", ")
        ));
        return tmp;
    }
    // An enum constructor is `Enum.Variant` (exactly one dot, e.g. `Option.Some`).
    // A module-qualified function call (e.g. `mvp_std.option.contains`,
    // `pkg.lib.foo`) has two or more dots and must be resolved as a function
    // via map_builtin, not treated as an enum constructor.
    if name.matches('.').count() == 1 {
        let dot = name.find('.').unwrap();
        let enum_name = &name[..dot];
        let variant = &name[dot + 1..];
        let arg_strs: Vec<String> = args
            .iter()
            .map(|a| format!("i64 {}", gen_expr(a, ctx, body)))
            .collect();
        let tmp = ctx.gen_tmp("ecall");
        if arg_strs.is_empty() {
            body.push_str(&format!(
                "{}{} = call i64 @{}_{}()\n",
                ctx.indent_str(),
                tmp,
                enum_name,
                variant
            ));
        } else {
            body.push_str(&format!(
                "{}{} = call i64 @{}_{}({})\n",
                ctx.indent_str(),
                tmp,
                enum_name,
                variant,
                arg_strs.join(", ")
            ));
        }
        return tmp;
    } else if let Some(enum_name) = args.first().and_then(|a| match a {
        Expr::EVar { name: n, .. } => Some(n.clone()),
        _ => None,
    }) {
        // Desugared method-call enum constructor: `Circle(Shape, 5)`
        // (from `Shape.Circle(5)`). Restrict by uppercase: enum type names
        // start uppercase in Miva (e.g. `Shape`), while variable names
        // (e.g. `circle`) are lowercase — so `area(circle)` never matches here.
        if enum_name.chars().next().map_or(false, |c| c.is_uppercase()) {
            let arg_strs: Vec<String> = args[1..]
                .iter()
                .map(|a| format!("i64 {}", gen_expr(a, ctx, body)))
                .collect();
            let tmp = ctx.gen_tmp("ecall");
            if arg_strs.is_empty() {
                body.push_str(&format!(
                    "{}{} = call i64 @{}_{}()\n",
                    ctx.indent_str(),
                    tmp,
                    enum_name,
                    name
                ));
            } else {
                body.push_str(&format!(
                    "{}{} = call i64 @{}_{}({})\n",
                    ctx.indent_str(),
                    tmp,
                    enum_name,
                    name,
                    arg_strs.join(", ")
                ));
            }
            return tmp;
        }
    }
    // string_from/to_string on already-string arg: skip conversion, pass through.
    // When the arg is NOT a string, still emit the conversion using the already
    // computed register — never re-evaluate args[0], which would double-evaluate
    // side-effecting expressions such as a second `await` on the same future.
    if (name == "string_from" || name == "to_string") && args.len() == 1 {
        let reg = gen_expr(&args[0], ctx, body);
        let is_str = is_string_expr(&args[0])
            || call_returns_string(&args[0], ctx)
            || is_string_var(&args[0], ctx)
            || ctx.string_regs.contains(&reg)
            || matches!(infer_expr_type(&args[0], ctx), Some(Typ::TString));
        if is_str {
            return reg;
        }
        // The value is numeric; choose the matching runtime conversion so the
        // result stringifies correctly (e.g. 0.1 not the raw bit-pattern).
        let tmp = ctx.gen_tmp("call");
        let (fn_name, arg_llvm) = match expr_numeric_kind(&args[0], ctx) {
            NumKind::Float => {
                let double_reg = if matches!(args[0], Expr::EFloat { .. }) {
                    reg
                } else {
                    let bc = ctx.gen_tmp("bc");
                    body.push_str(&format!(
                        "{}{} = bitcast i64 {} to double\n",
                        ctx.indent_str(),
                        bc,
                        reg
                    ));
                    bc
                };
                (
                    "@miva_string_from_float".to_string(),
                    format!("double {}", double_reg),
                )
            }
            NumKind::Bool => {
                let b = ctx.gen_tmp("bt");
                body.push_str(&format!(
                    "{}{} = trunc i64 {} to i8\n",
                    ctx.indent_str(),
                    b,
                    reg
                ));
                ("@miva_string_from_bool".to_string(), format!("i8 {}", b))
            }
            NumKind::Int => ("@miva_string_from_int".to_string(), format!("i64 {}", reg)),
        };
        body.push_str(&format!(
            "{} = call ptr {}({})\n",
            format!("{}{}", ctx.indent_str(), tmp),
            fn_name,
            arg_llvm
        ));
        let int_tmp = ctx.gen_tmp("cr");
        body.push_str(&format!(
            "{}{} = ptrtoint ptr {} to i64\n",
            ctx.indent_str(),
            int_tmp,
            tmp
        ));
        ctx.string_regs.insert(int_tmp.clone());
        return int_tmp;
    }

    // `await(f)` / `f.await()`: join the spawned task and return its value.
    if name == "await" {
        let arg = match args.first() {
            Some(a) => a,
            None => return String::new(),
        };
        let arg_reg = gen_expr(arg, ctx, body);
        let tmp = ctx.gen_tmp("call");
        body.push_str(&format!(
            "{} = call i64 @miva_async_await(i64 {})\n",
            format!("{}{}", ctx.indent_str(), tmp),
            arg_reg
        ));
        // The awaited value is the future's inner type; mirror string-ness.
        if call_returns_string(arg, ctx) || is_string_expr(arg) || is_string_var(arg, ctx) {
            ctx.string_regs.insert(tmp.clone());
        }
        return tmp;
    }

    // Spawn a real OS thread for calls to async functions. The async function
    // takes a single `i64` (pointer to a heap struct of packed args); the
    // runtime spawns it and returns a task handle (also an i64).
    let lookup = name.rsplit('.').next().unwrap_or(name);
    let is_async_call = ctx.func_sigs.get(lookup).map_or(false, |s| s.is_async);
    if is_async_call {
        let arg_regs: Vec<String> = args.iter().map(|a| gen_expr(a, ctx, body)).collect();
        let struct_bytes = (arg_regs.len() as i64) * 8;
        let struct_ptr = ctx.gen_tmp("asp");
        body.push_str(&format!(
            "{}{} = call ptr @miva_alloc(i64 {})\n",
            ctx.indent_str(),
            struct_ptr,
            struct_bytes
        ));
        for (i, reg) in arg_regs.iter().enumerate() {
            let gep = ctx.gen_tmp("asg");
            body.push_str(&format!(
                "{}{} = getelementptr i64, ptr {}, i64 {}\n",
                ctx.indent_str(),
                gep,
                struct_ptr,
                i
            ));
            body.push_str(&format!(
                "{}store i64 {}, ptr {}\n",
                ctx.indent_str(),
                reg,
                gep
            ));
        }
        let struct_int = ctx.gen_tmp("asi");
        body.push_str(&format!(
            "{}{} = ptrtoint ptr {} to i64\n",
            ctx.indent_str(),
            struct_int,
            struct_ptr
        ));
        let func_name = map_builtin(name, ctx.current_module.as_deref());
        let handle = ctx.gen_tmp("call");
        body.push_str(&format!(
            "{} = call i64 @miva_async_spawn(ptr {}, i64 {})\n",
            format!("{}{}", ctx.indent_str(), handle),
            func_name,
            struct_int
        ));
        // The handle is a plain i64; it is NOT a string. String-ness is applied
        // by the `await` that later joins this handle.
        return handle;
    }

    let func_name = map_builtin(name, ctx.current_module.as_deref());

    let mut arg_strs = Vec::new();
    for (i, a) in args.iter().enumerate() {
        let t = gen_expr(a, ctx, body);
        if is_ptr_arg(&func_name, i) {
            let ptr_val = ctx.gen_tmp("sp");
            body.push_str(&format!(
                "{}{} = inttoptr i64 {} to ptr\n",
                ctx.indent_str(),
                ptr_val,
                t
            ));
            arg_strs.push(format!("ptr {}", ptr_val));
        } else {
            arg_strs.push(format!("i64 {}", t));
        }
    }

    if !func_name.starts_with("@miva_") && !func_name.starts_with("@ffi_") {
        if let Ok(mut guard) = EXTERN_DECLS.lock() {
            let bare = func_name.trim_start_matches('@').to_string();
            let param_list = arg_strs
                .iter()
                .map(|s| s.split_whitespace().next().unwrap_or("i64").to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let decl = format!("declare i64 @{}({})", bare, param_list);
            guard.get_or_insert_with(HashSet::new).insert(decl);
        }
    }
    let ret_ty = ret_type(&func_name);
    if ret_ty == "void" {
        body.push_str(&format!(
            "{}call void {}({})\n",
            ctx.indent_str(),
            func_name,
            arg_strs.join(", ")
        ));
        "0".to_string()
    } else {
        let tmp = ctx.gen_tmp("call");
        body.push_str(&format!(
            "{}{} = call {} {}({})\n",
            ctx.indent_str(),
            tmp,
            ret_ty,
            func_name,
            arg_strs.join(", ")
        ));
        let result = if ret_ty == "ptr" {
            let int_tmp = ctx.gen_tmp("cr");
            body.push_str(&format!(
                "{}{} = ptrtoint ptr {} to i64\n",
                ctx.indent_str(),
                int_tmp,
                tmp
            ));
            // Mark ptr-to-i64 converted register as string
            ctx.string_regs.insert(int_tmp.clone());
            int_tmp
        } else {
            // For user functions, check signature to see if result is a string
            let lookup = name.rsplit('.').next().unwrap_or(name);
            if !func_name.starts_with("@miva_") && !func_name.starts_with("@ffi_") {
                if let Some(sig) = ctx.func_sigs.get(lookup) {
                    if returns_from_sig(sig, type_args) {
                        ctx.string_regs.insert(tmp.clone());
                    }
                }
            }
            tmp
        };
        result
    }
}

/// Check if a FuncSig indicates string return given concrete type_args.
pub(crate) fn returns_from_sig(sig: &crate::codegen::FuncSig, type_args: &[Typ]) -> bool {
    match &sig.returns {
        Some(Typ::TString) => true,
        Some(Typ::TFuture { of }) => {
            if let Typ::TString = **of {
                return true;
            }
            false
        }
        Some(Typ::TStruct { name, .. }) => {
            if let Some(pos) = sig.type_params.iter().position(|p| p == name) {
                if pos < type_args.len() {
                    return matches!(&type_args[pos], Typ::TString);
                }
            }
            false
        }
        _ => false,
    }
}

/// Decide whether binding `name` to `expr` yields a string value, so the
/// backend stringifies it with `string_from_str` rather than `string_from_int`.
/// Extends the existing checks with field-access into an enum value whose
/// payload at that index is known to carry a string, and field-access into a
/// struct/shape field whose type is `TString`.
pub(crate) fn binding_is_string(expr: &Expr, ctx: &LlvmCtx) -> bool {
    if is_string_expr(expr) || call_returns_string(expr, ctx) {
        return true;
    }
    if matches!(infer_expr_type(expr, ctx), Some(Typ::TString)) {
        return true;
    }
    // `v = scrutinee.field{i}` where `scrutinee` holds an enum value with a
    // string at payload index `i` (e.g. the `v` in `when (Box.Value(v))`).
    if let Expr::EFieldAccess {
        expr: base, field, ..
    } = expr
    {
        if field.chars().all(|c| c.is_ascii_digit()) {
            if let Expr::EVar { name, .. } = base.as_ref() {
                if let Some(idxs) = ctx.string_payloads.get(name) {
                    if idxs.contains(&field.parse::<usize>().unwrap_or(usize::MAX)) {
                        return true;
                    }
                }
            }
        }
        // `v = struct.field` where `field` is typed as `string` in the struct.
        if let Expr::EVar { name: var_name, .. } = base.as_ref() {
            if let Some(typ) = ctx.var_types.get(var_name) {
                let struct_name = match typ {
                    Typ::TStruct { name, .. } => name.as_str(),
                    Typ::TShape { name, .. } => name.as_str(),
                    _ => "",
                };
                if let Some(field_types) = ctx.struct_field_types.get(struct_name) {
                    if let Some(field_typ) = field_types.get(field) {
                        if typ_is_string(field_typ) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Record, for a variable that is being bound to an enum constructor value,
/// which of its payload indices carry strings (for later destructuring).
pub(crate) fn record_string_payloads(name: &str, expr: &Expr, ctx: &mut LlvmCtx) {
    let idxs = match expr {
        Expr::ECall {
            name: cname,
            type_args,
            ..
        } => {
            if let Some(dot) = cname.find('.') {
                let (en, variant) = (&cname[..dot], &cname[dot + 1..]);
                enum_ctor_string_payloads(en, variant, type_args, &ctx.enum_defs)
            } else if let Some(en) = enum_ctor_enum_name(expr) {
                if en.chars().next().map_or(false, |c| c.is_uppercase()) {
                    enum_ctor_string_payloads(&en, cname, type_args, &ctx.enum_defs)
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    };
    if !idxs.is_empty() {
        ctx.string_payloads.insert(name.to_string(), idxs);
    }
}

/// Get the enum type name from the first argument of a desugared
/// `Variant(Enum, ...)` enum-constructor call.
pub(crate) fn enum_ctor_enum_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::ECall { args, .. } => match args.first() {
            Some(Expr::EVar { name, .. }) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}
