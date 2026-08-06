use super::*;

pub(crate) fn is_string_expr(expr: &Expr) -> bool {
    match expr {
        Expr::EString { .. } => true,
        Expr::ECall { name, .. } => {
            matches!(
                name.as_str(),
                "string_from" | "string_concat" | "string_make" | "to_string"
            )
        }
        Expr::EBinOp {
            op: BinOp::Add,
            left,
            right,
            ..
        } => is_string_expr(left) || is_string_expr(right),
        _ => false,
    }
}

/// Check if an ECall returns a string based on cross-file function signature.
pub(crate) fn call_returns_string(expr: &Expr, ctx: &LlvmCtx) -> bool {
    match expr {
        Expr::ECall {
            name, type_args, ..
        } => {
            let lookup = name.rsplit('.').next().unwrap_or(name);
            ctx.func_sigs
                .get(lookup)
                .map_or(false, |sig| returns_from_sig(sig, type_args))
        }
        _ => false,
    }
}

/// Check if an expression is an enum value (discriminant `Shape.Circle` or
/// constructor `Shape.Circle(5)` / desugared `Circle(Shape, 5)`). Used so
/// `==`/`!=` compares enum tags rather than heap pointer addresses.
pub(crate) fn is_enum_value_expr(expr: &Expr) -> bool {
    match expr {
        Expr::EFieldAccess { field, expr, .. } => {
            !field.chars().all(|c| c.is_ascii_digit())
                && matches!(expr.as_ref(), Expr::EVar { name, .. } if name.chars().next().map_or(false, |c| c.is_uppercase()))
        }
        Expr::ECall { name, args, .. } => {
            if name.matches('.').count() == 1 {
                true
            } else if let Some(Expr::EVar { name: n, .. }) = args.first() {
                n.chars().next().map_or(false, |c| c.is_uppercase())
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Whether a field access into `base_expr` with `field_name` yields a string.
pub(crate) fn field_access_is_string(base_expr: &Expr, field_name: &str, ctx: &LlvmCtx) -> bool {
    if let Expr::EVar { name: var_name, .. } = base_expr {
        if let Some(typ) = ctx.var_types.get(var_name) {
            let struct_name = match typ {
                Typ::TStruct { name, .. } => name.as_str(),
                Typ::TShape { name, .. } => name.as_str(),
                _ => "",
            };
            if let Some(field_types) = ctx.struct_field_types.get(struct_name) {
                if let Some(field_typ) = field_types.get(field_name) {
                    return typ_is_string(field_typ);
                }
            }
        }
    }
    false
}

/// Load the `__tag` field (at offset 0) of an enum value held in `reg` and
/// return the i64 tag register name.
pub(crate) fn load_enum_tag(ctx: &mut LlvmCtx, body: &mut String, reg: &str) -> String {
    let ptr = ctx.gen_tmp("etp");
    body.push_str(&format!(
        "{}{} = inttoptr i64 {} to ptr\n",
        ctx.indent_str(),
        ptr,
        reg
    ));
    let gep = ctx.gen_tmp("etg");
    body.push_str(&format!(
        "{}{} = getelementptr i64, ptr {}, i64 0\n",
        ctx.indent_str(),
        gep,
        ptr
    ));
    let load = ctx.gen_tmp("etl");
    body.push_str(&format!(
        "{}{} = load i64, ptr {}\n",
        ctx.indent_str(),
        load,
        gep
    ));
    load
}

/// Check if an EVar/EMove refers to a register known to hold a string.
pub(crate) fn is_string_var(expr: &Expr, ctx: &LlvmCtx) -> bool {
    match expr {
        Expr::EVar { name, .. } | Expr::EMove { name, .. } => ctx
            .var_reloads
            .get(name)
            .map_or(false, |r| ctx.string_regs.contains(r)),
        _ => false,
    }
}

/// Substitute generic type parameters in `t` using `subst` (param name -> type).
/// Generic params appear in enum payloads either as `TGenericParam` or as a
/// `TStruct` with no fields whose name is the parameter (the frontend's
/// representation for a bare type parameter).
pub(crate) fn resolve_enum_typ(t: &Typ, subst: &HashMap<String, Typ>) -> Typ {
    match t {
        Typ::TGenericParam { name } => subst.get(name).cloned().unwrap_or_else(|| t.clone()),
        Typ::TStruct {
            name,
            fields,
            type_args,
        } if fields.is_empty() && type_args.is_empty() => {
            subst.get(name).cloned().unwrap_or_else(|| t.clone())
        }
        Typ::TArray { of } => Typ::TArray {
            of: Box::new(resolve_enum_typ(of, subst)),
        },
        _ => t.clone(),
    }
}

/// Whether a (resolved) type is, or holds, a string value.
pub(crate) fn typ_is_string(t: &Typ) -> bool {
    matches!(t, Typ::TString) || matches!(t, Typ::TArray { of } if matches!(**of, Typ::TString))
}

/// Substitute concrete type arguments for generic parameters throughout `t`.
pub(crate) fn subst_type_params(t: &Typ, params: &[String], args: &[Typ]) -> Typ {
    match t {
        Typ::TGenericParam { name } => params
            .iter()
            .position(|p| p == name)
            .and_then(|i| args.get(i))
            .cloned()
            .unwrap_or_else(|| t.clone()),
        Typ::TStruct {
            name,
            fields,
            type_args,
        } if fields.is_empty() && type_args.is_empty() => params
            .iter()
            .position(|p| p == name)
            .and_then(|i| args.get(i))
            .cloned()
            .unwrap_or_else(|| t.clone()),
        Typ::TStruct {
            name,
            fields,
            type_args,
        } => Typ::TStruct {
            name: name.clone(),
            fields: fields.clone(),
            type_args: type_args
                .iter()
                .map(|a| subst_type_params(a, params, args))
                .collect(),
        },
        Typ::TArray { of } => Typ::TArray {
            of: Box::new(subst_type_params(of, params, args)),
        },
        Typ::TTuple { elems } => Typ::TTuple {
            elems: elems
                .iter()
                .map(|e| subst_type_params(e, params, args))
                .collect(),
        },
        Typ::TFuture { of } => Typ::TFuture {
            of: Box::new(subst_type_params(of, params, args)),
        },
        Typ::TBox { of } => Typ::TBox {
            of: Box::new(subst_type_params(of, params, args)),
        },
        _ => t.clone(),
    }
}

/// Best-effort type inference for an expression, used to make codegen
/// decisions that the sparse `var_types` table cannot answer reliably (SLet
/// bindings, tuple field access, string/bool payload printing). Returns `None`
/// when the type is not statically known.
pub(crate) fn infer_expr_type(expr: &Expr, ctx: &LlvmCtx) -> Option<Typ> {
    match expr {
        Expr::EInt { .. } => Some(Typ::TInt),
        Expr::EBool { .. } => Some(Typ::TBool),
        Expr::EChar { .. } => Some(Typ::TChar),
        Expr::EFloat { .. } => Some(Typ::TFloat64),
        Expr::EString { .. } => Some(Typ::TString),
        Expr::EVar { name, .. } | Expr::EMove { name, .. } => ctx.var_types.get(name).cloned(),
        Expr::ETupleLit { values, .. } => Some(Typ::TTuple {
            elems: values
                .iter()
                .map(|v| infer_expr_type(v, ctx).unwrap_or(Typ::TInt))
                .collect(),
        }),
        Expr::EStructLit {
            name, type_args, ..
        } => Some(Typ::TStruct {
            name: name.clone(),
            fields: vec![],
            type_args: type_args.clone(),
        }),
        Expr::EArrayLit { values, .. } => values
            .first()
            .and_then(|v| infer_expr_type(v, ctx))
            .map(|of| Typ::TArray { of: Box::new(of) }),
        Expr::ECall {
            name,
            type_args,
            args,
            ..
        } => {
            // Enum constructor `Enum.Variant` (or desugared `Variant(Enum, ..)`):
            // the value has the enum's type.
            if name.matches('.').count() == 1 {
                let enum_name = &name[..name.find('.').unwrap()];
                if ctx.enum_defs.contains_key(enum_name) {
                    return Some(Typ::TStruct {
                        name: enum_name.to_string(),
                        fields: vec![],
                        type_args: vec![],
                    });
                }
            }
            if let Some(Expr::EVar { name: n, .. }) = args.first() {
                if n.chars().next().map_or(false, |c| c.is_uppercase())
                    && ctx.enum_defs.contains_key(n)
                {
                    return Some(Typ::TStruct {
                        name: n.clone(),
                        fields: vec![],
                        type_args: vec![],
                    });
                }
            }
            let lookup = name.rsplit('.').next().unwrap_or(name);
            ctx.func_sigs.get(lookup).and_then(|sig| {
                sig.returns
                    .as_ref()
                    .map(|r| subst_type_params(r, &sig.type_params, type_args))
            })
        }
        Expr::EFieldAccess {
            expr: base, field, ..
        } => infer_field_type(base, field, ctx),
        Expr::ELambda { params, ret, .. } => Some(Typ::TFunc {
            params: params
                .iter()
                .map(|p| match p {
                    Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ.clone(),
                })
                .collect(),
            returns: Box::new(ret.clone()),
        }),
        Expr::ECast { to, .. } => Some(to.clone()),
        Expr::EBinOp {
            op: BinOp::Add,
            left,
            right,
            ..
        } => {
            if is_string_expr(left) || is_string_expr(right) {
                Some(Typ::TString)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Type of the value produced by `base.field`. Handles tuple element indices
/// and struct/shape fields (via the struct field type table).
pub(crate) fn infer_field_type(base: &Expr, field: &str, ctx: &LlvmCtx) -> Option<Typ> {
    match infer_expr_type(base, ctx) {
        Some(Typ::TTuple { elems }) => field
            .parse::<usize>()
            .ok()
            .and_then(|i| elems.get(i))
            .cloned(),
        Some(Typ::TStruct { name, .. }) => ctx
            .struct_field_types
            .get(&name)
            .and_then(|m| m.get(field))
            .cloned(),
        Some(Typ::TShape { name, .. }) => ctx
            .struct_field_types
            .get(&name)
            .and_then(|m| m.get(field))
            .cloned(),
        _ => None,
    }
}

/// Whether `expr` is a tuple or (non-enum) struct value, returning its type.
/// Used to decide when `==`/`!=` needs a deep, per-field comparison.
pub(crate) fn aggregate_compare_type(l: &Expr, r: &Expr, ctx: &LlvmCtx) -> Option<Typ> {
    let t = infer_expr_type(l, ctx).or_else(|| infer_expr_type(r, ctx))?;
    match &t {
        Typ::TTuple { .. } => Some(t),
        Typ::TStruct { name, .. } if !ctx.enum_defs.contains_key(name) => Some(t),
        _ => None,
    }
}

/// Given a type that names an enum (e.g. `Box[string]`), return the payload
/// indices of that enum that carry strings for this concrete instantiation.
/// Returns `None` when the type does not name a known enum.
pub(crate) fn enum_string_payloads_from_typ(
    typ: &Typ,
    enum_defs: &HashMap<String, (Vec<String>, HashMap<String, Vec<Typ>>)>,
) -> Option<Vec<usize>> {
    let (enum_name, type_args) = match typ {
        Typ::TStruct {
            name, type_args, ..
        } => (name.clone(), type_args.clone()),
        _ => return None,
    };
    let (params, variants) = enum_defs.get(&enum_name)?;
    let mut subst = HashMap::new();
    for (p, a) in params.iter().zip(type_args.iter()) {
        subst.insert(p.clone(), a.clone());
    }
    let mut all = Vec::new();
    for payload in variants.values() {
        for (i, t) in payload.iter().enumerate() {
            if typ_is_string(&resolve_enum_typ(t, &subst)) {
                all.push(i);
            }
        }
    }
    Some(all)
}

/// Compute the payload indices of an enum constructor call that carry strings,
/// given the enum definition and the concrete type arguments (if the enum is
/// generic). Returns an empty vec when the constructor is unknown.
pub(crate) fn enum_ctor_string_payloads(
    enum_name: &str,
    variant: &str,
    type_args: &[Typ],
    enum_defs: &HashMap<String, (Vec<String>, HashMap<String, Vec<Typ>>)>,
) -> Vec<usize> {
    let Some((params, variants)) = enum_defs.get(enum_name) else {
        return Vec::new();
    };
    let Some(payload) = variants.get(variant) else {
        return Vec::new();
    };
    let mut subst = HashMap::new();
    for (p, a) in params.iter().zip(type_args.iter()) {
        subst.insert(p.clone(), a.clone());
    }
    payload
        .iter()
        .enumerate()
        .filter(|(_, t)| typ_is_string(&resolve_enum_typ(t, &subst)))
        .map(|(i, _)| i)
        .collect()
}

/// Numeric category of a value, used to pick the correct `string_from_*`
/// runtime conversion when stringifying a non-string value.
#[derive(Debug)]
pub(crate) enum NumKind {
    Int,
    Float,
    Bool,
}
pub(crate) fn expr_numeric_kind(expr: &Expr, ctx: &LlvmCtx) -> NumKind {
    match expr {
        Expr::EFloat { .. } => NumKind::Float,
        Expr::EInt { .. } => NumKind::Int,
        Expr::EBool { .. } => NumKind::Bool,
        Expr::EVar { name, .. } => match ctx.var_types.get(name) {
            Some(Typ::TBool) => NumKind::Bool,
            Some(Typ::TFloat64) | Some(Typ::TFloat32) => NumKind::Float,
            _ => NumKind::Int,
        },
        Expr::ECall { name, .. } => {
            let lookup = name.rsplit('.').next().unwrap_or(name);
            match ctx.func_sigs.get(lookup) {
                Some(sig) => match &sig.returns {
                    Some(Typ::TFloat64) | Some(Typ::TFloat32) => NumKind::Float,
                    Some(Typ::TBool) => NumKind::Bool,
                    _ => NumKind::Int,
                },
                None => NumKind::Int,
            }
        }
        Expr::ETupleLit { .. } => NumKind::Int,
        Expr::EFieldAccess {
            expr: base, field, ..
        } => match infer_field_type(base, field, ctx) {
            Some(Typ::TBool) => NumKind::Bool,
            Some(Typ::TFloat64) | Some(Typ::TFloat32) => NumKind::Float,
            _ => NumKind::Int,
        },
        _ => NumKind::Int,
    }
}

pub(crate) fn emit_fresh_loads(
    ctx: &mut LlvmCtx,
    body: &mut String,
    before: &HashMap<String, String>,
    names: &[String],
) {
    for name in names {
        let addr = ctx.get_var_addr(name);
        // Use the per-variable seq counter so we stay inside the same
        // `s.r.N` namespace that `declare_var` and `SAssign` use, avoiding
        // collisions with `tmp_counter`-derived names.
        let count = ctx.var_seq.entry(name.to_string()).or_insert(0);
        let new_reload = format!("{}.r.{}", name, count);
        *count += 1;
        body.push_str(&format!(
            "  %{} = load i64, ptr %{}, align 8\n",
            new_reload, addr
        ));
        ctx.var_reloads.insert(name.clone(), new_reload.clone());
        if before
            .get(name)
            .map_or(false, |r| ctx.string_regs.contains(r))
        {
            ctx.string_regs.insert(new_reload);
        }
    }
}

/// Whether the last non-empty instruction emitted into `body` is a block
/// terminator (a `ret` or `br`). Used to avoid emitting a fall-through
/// `br` (and a malformed merge `phi`) after a branch that already returns.
pub(crate) fn body_ends_in_terminator(body: &str) -> bool {
    for line in body.lines().rev() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        return t.starts_with("ret ") || t.starts_with("br ");
    }
    false
}

/// Detect the most recently emitted `ifend` (merge) label in `body` if the
/// last block (between the most recent label and the end of `body`) contains
/// a phi instruction. Returns the label for the phi predecessor to use when
/// a nested EIf's merge must be the source of a phi at the outer EIf.
/// `entry_label` is the label of the block we just emitted (`label_else`);
/// if no inner merge was found, we fall back to that.
pub(crate) fn detect_last_merge_label(body: &str, entry_label: String) -> (String, Option<String>) {
    // Walk the body. Track the label of the most recently entered block and
    // whether that block contained a phi. The current "last block" is the
    // one we're inside right now (between its label and the end of body).
    let mut last_block_label: Option<String> = None;
    let mut last_block_has_phi = false;
    for line in body.lines() {
        let t = line.trim();
        // A new label (e.g. `ifend_28:`) starts a new block. The previous
        // block (if any) is now closed — we don't need it for this query,
        // but its contents are correctly attributed to it.
        if t.ends_with(':') && !t.starts_with(';') && !t.starts_with('%') {
            last_block_label = Some(t.trim_end_matches(':').to_string());
            last_block_has_phi = false;
        } else if t.starts_with("phi ") || t.starts_with("%phi ") || t.contains(" phi ") {
            // Phi instructions may be emitted as `  %phi_37 = phi i64 ...`
            // (LLVM textual form) or `phi i64 ...` (debug form). The
            // register name is a percent-prefixed local; strip it for
            // detection.
            last_block_has_phi = true;
        }
        // We deliberately ignore `ret`/`br` here: those are terminators of
        // earlier blocks, not of the current (open) trailing block. The
        // current trailing block can still have its own phi + reloads.
    }
    if last_block_has_phi {
        if let Some(lbl) = last_block_label {
            if lbl != entry_label {
                return (entry_label, Some(lbl));
            }
        }
    }
    (entry_label, None)
}

/// Append a `br label %target` at the end of the merge block identified by
/// `merge_label`. The merge block contains a phi followed by zero or more
/// reload instructions but no terminator; we splice the new br in just
/// before the next label/empty line, or at the end of body if no marker
/// follows. This is the LLVM-correct position: a br is a block terminator
/// and must be the final instruction of its block.
pub(crate) fn append_br_after_merge(body: &mut String, merge_label: &str, target: &str) {
    let lines: Vec<String> = body.lines().map(|s| s.to_string()).collect();
    let mut out = String::new();
    let mut in_merge = false;
    let mut inserted = false;
    for line in lines {
        let t = line.trim();
        if t == format!("{}:", merge_label) {
            in_merge = true;
            out.push_str(&line);
            out.push('\n');
            continue;
        }
        if in_merge && !inserted {
            // A new label, an empty line, or another terminator means the
            // merge block is over — insert the br right before it.
            if t.is_empty() || (t.ends_with(':') && !t.starts_with('%')) {
                out.push_str(&format!("  br label %{}\n", target));
                inserted = true;
                in_merge = false;
            }
        }
        out.push_str(&line);
        out.push('\n');
    }
    if in_merge && !inserted {
        // Merge block was the very last thing in body, with no following
        // marker. Append the br to terminate the block.
        out.push_str(&format!("  br label %{}\n", target));
    }
    *body = out;
}
