use std::collections::{HashMap, HashSet};

use crate::ast::*;

use miva_vm::vm::{MvmFunction, MvmProgram};
use miva_vm::Opcode as MvmOp;

/// Label management for jump resolution.
struct Label {
    /// Absolute code position where this label is defined (None if not yet defined).
    pos: Option<usize>,
    /// List of (code_position, is_if_not) pairs that need patching.
    pending: Vec<(usize, bool)>,
}

/// A user `unsafe fn` whose body is raw C, implemented in the project's
/// single `libhost.so`. Emitted as a `CallHost` at call sites and collected
/// so the build step can generate the matching C shim.
pub struct HostDef {
    pub name: String,
    pub arity: u32,
    pub returns: Option<Typ>,
    pub code: String,
}

/// A lambda (closure) thunk registered during expression compilation and
/// compiled into a fresh function-table slot after the enclosing function's
/// body is generated. The thunk's first `captures.len()` locals hold the
/// captured environment values (in `captures` order); the remaining locals
/// hold its own parameters.
struct ClosureThunk {
    captures: Vec<(String, Typ)>,
    params: Vec<Param>,
    body: Expr,
    ret: Typ,
    func_idx: usize,
}

/// Miva VM bytecode code generator.
pub struct MvmCodegen {
    // --- String pool ---
    string_pool: Vec<String>,
    string_indices: HashMap<String, u32>,

    // --- Function table ---
    functions: Vec<MvmFunction>,
    func_indices: HashMap<String, usize>,
    builtin_indices: HashMap<String, u8>,
    /// Names of user `unsafe fn`s implemented as host functions in libhost.so.
    host_funcs: HashSet<String>,
    /// For each function, the list of PRef parameter names.
    func_ref_params: HashMap<String, Vec<String>>,
    /// Collected user `unsafe fn` definitions (raw C) for libhost.so generation.
    host_defs: Vec<HostDef>,
    /// For each function that is `void` AND has at least one PRef parameter,
    /// the list of its PRef parameter names. Such functions return the mutated
    /// struct to the caller, which then stores it back into the arg's local.
    /// Non-void ref-param functions (e.g. `get`, `len`, `pop`) return their real
    /// result and must NOT trigger the caller-side store-back.
    void_ref_params: HashMap<String, Vec<String>>,

    // --- Struct field maps ---
    struct_field_indices: HashMap<String, Vec<(String, usize)>>, // struct name -> [(field_name, index)]
    /// struct/shape name -> typed field defs (for resolving nested field access chains)
    struct_defs: HashMap<String, Vec<FieldDef>>,

    // --- Impl table (operator overloading) ---
    impl_map: HashMap<String, HashMap<String, String>>, // struct_name -> { op_name -> func_name }

    // --- Current function compilation state ---
    code: Vec<u8>,
    /// Opcode of the most recently emitted instruction (None before any emit).
    /// Used to decide whether a trailing `Ret`/`RetVal` already terminates the
    /// function body, so we don't double-return or misread a multi-byte
    /// instruction's operand byte as a terminal opcode.
    last_emitted: Option<MvmOp>,
    locals_count: u32,
    local_indices: HashMap<String, Vec<u32>>, // name -> stack of indices (handles shadowing)
    scope_stack: Vec<u32>,
    param_types: HashMap<String, Typ>, // parameter name -> type
    /// Set of local indices that hold reference pointers (PRef params).
    ptr_params: HashSet<u32>,

    // --- Label management ---
    labels: HashMap<u32, Label>,
    next_label: u32,

    // --- Current function info ---
    current_func_name: String,

    // --- Closure support ---
    /// For each variable name, its static type. Used to detect closure-typed
    /// variables at call sites so they are invoked via `CallClosure` rather
    /// than the ordinary `Call` (function-index) path.
    var_types: HashMap<String, Typ>,
    /// Lambda thunks registered during expression compilation, flushed into
    /// the function table once the enclosing function body is complete.
    pending_thunks: Vec<ClosureThunk>,
}

impl MvmCodegen {
    pub fn new() -> Self {
        let mut builtin_indices = HashMap::new();
        for (name, idx) in miva_vm::builtins::BUILTIN_IDS {
            builtin_indices.insert(name.to_string(), *idx);
        }
        MvmCodegen {
            string_pool: Vec::new(),
            string_indices: HashMap::new(),
            functions: Vec::new(),
            func_indices: HashMap::new(),
            builtin_indices,
            host_funcs: HashSet::new(),
            func_ref_params: HashMap::new(),
            host_defs: Vec::new(),
            void_ref_params: HashMap::new(),
            struct_field_indices: HashMap::new(),
            struct_defs: HashMap::new(),
            impl_map: HashMap::new(),
            code: Vec::new(),
            last_emitted: None,
            locals_count: 0,
            local_indices: HashMap::new(),
            scope_stack: Vec::new(),
            param_types: HashMap::new(),
            ptr_params: HashSet::new(),
            labels: HashMap::new(),
            next_label: 0,
            current_func_name: String::new(),
            var_types: HashMap::new(),
            pending_thunks: Vec::new(),
        }
    }
    fn resolve_string(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.string_indices.get(s) {
            return idx;
        }
        let idx = self.string_pool.len() as u32;
        self.string_pool.push(s.to_string());
        self.string_indices.insert(s.to_string(), idx);
        idx
    }

    // --- Bytecode emitter helpers ---

    fn emit_u8(&mut self, b: u8) {
        self.code.push(b);
    }

    fn emit_u32(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    fn emit_i32(&mut self, v: i32) {
        let before = self.code.len();
        self.code.extend_from_slice(&v.to_le_bytes());
        let after = self.code.len();
        if after - before != 4 {
        }
    }

    fn emit_i64(&mut self, v: i64) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    fn emit_f64(&mut self, v: f64) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    // --- Label management ---
    fn emit_op(&mut self, op: MvmOp) {
        self.code.push(op as u8);
        self.last_emitted = Some(op);
    }

    fn new_label(&mut self) -> u32 {
        let id = self.next_label;
        self.next_label += 1;
        self.labels.insert(id, Label { pos: None, pending: Vec::new() });
        id
    }

    fn define_label(&mut self, id: u32) {
        let pos = self.code.len();
        let label = self.labels.get_mut(&id).expect("label not found");
        label.pos = Some(pos);
        // Resolve pending jumps targeting this label
        let pending = std::mem::take(&mut label.pending);
        for (jump_pos, _is_if_not) in pending {
            let offset = pos as i32 - jump_pos as i32 - 4; // -4 for the offset field itself
            let offset_bytes = offset.to_le_bytes();
            self.code[jump_pos..jump_pos + 4].copy_from_slice(&offset_bytes);
        }
    }

    fn emit_jmp(&mut self, target_label: u32) {
        let pos = self.code.len();
        self.emit_op(MvmOp::Jmp);
        self.emit_i32(0); // placeholder
        let label = self.labels.get_mut(&target_label).unwrap();
        if let Some(target_pos) = label.pos {
            // offset relative to end of instruction (pos + 5 = next instruction position)
            let offset = target_pos as i32 - pos as i32 - 5;
            let len = self.code.len();
            self.code[len - 4..len].copy_from_slice(&offset.to_le_bytes());
        } else {
            label.pending.push((pos + 1, false)); // +1 to skip opcode byte
        }
    }

    fn emit_jmp_if_true(&mut self, target_label: u32) {
        let pos = self.code.len();
        self.emit_op(MvmOp::JmpIf);
        self.emit_i32(0); // placeholder
        let label = self.labels.get_mut(&target_label).unwrap();
        if let Some(target_pos) = label.pos {
            let offset = target_pos as i32 - pos as i32 - 5;
            let len = self.code.len();
            self.code[len - 4..len].copy_from_slice(&offset.to_le_bytes());
        } else {
            label.pending.push((pos + 1, true));
        }
    }

    fn emit_jmp_if_false(&mut self, target_label: u32) {
        let pos = self.code.len();
        self.emit_op(MvmOp::JmpIfNot);
        self.emit_i32(0); // placeholder
        let label = self.labels.get_mut(&target_label).unwrap();
        if let Some(target_pos) = label.pos {
            let offset = target_pos as i32 - pos as i32 - 5;
            let len = self.code.len();
            self.code[len - 4..len].copy_from_slice(&offset.to_le_bytes());
        } else {
            label.pending.push((pos + 1, false));
        }
    }

    // --- Variable scope management ---

    fn push_scope(&mut self) {
        self.scope_stack.push(self.locals_count);
    }

    fn pop_scope(&mut self) {
        let _old_count = self.scope_stack.pop().expect("scope stack underflow");
        // Don't reset locals_count — variables from nested scopes stay
        // in the function's frame (they are just unused slots after scope exit).
        // Only clean up the name-to-index mapping so the names are no longer
        // accessible. The slots remain allocated for simplicity.
        self.local_indices.retain(|_, indices| {
            indices.retain(|&idx| idx < _old_count);
            !indices.is_empty()
        });
    }

    fn declare_local(&mut self, name: &str) -> u32 {
        let idx = self.locals_count;
        self.locals_count += 1;
        self.local_indices.entry(name.to_string())
            .or_default()
            .push(idx as u32);
        idx as u32
    }

    fn resolve_local(&self, name: &str) -> Option<u32> {
        self.local_indices.get(name)
            .and_then(|indices| indices.last().copied())
    }

    // --- Struct field info (populated during initialization) ---

    fn collect_struct_info(&mut self, defs: &[Def]) {
        for def in defs {
            match def {
                Def::DStruct { name, fields, .. } => {
                    let indexed: Vec<(String, usize)> = fields.iter()
                        .enumerate()
                        .map(|(i, f)| (f.name.clone(), i))
                        .collect();
                    self.struct_field_indices.insert(name.clone(), indexed);
                    self.struct_defs.insert(name.clone(), fields.clone());
                }
                Def::DShape { name, fields, .. } => {
                    // Shapes are treated similarly to structs for runtime field access
                    let indexed: Vec<(String, usize)> = fields.iter()
                        .enumerate()
                        .map(|(i, f)| (f.name.clone(), i))
                        .collect();
                    self.struct_field_indices.insert(name.clone(), indexed);
                    self.struct_defs.insert(name.clone(), fields.clone());
                }
                Def::DEnum { name, variants, .. } => {
                    let indexed: Vec<(String, usize)> = variants.iter()
                        .enumerate()
                        .map(|(i, v)| (v.name.clone(), i))
                        .collect();
                    self.struct_field_indices.insert(name.clone(), indexed);
                }
                Def::DImpl { struct_name, impls, .. } => {
                    let ops = self.impl_map.entry(struct_name.clone()).or_default();
                    for impl_expr in impls {
                        let op_name = match impl_expr.op {
                            ImplOp::ImAdd => "add",
                            ImplOp::ImSub => "sub",
                            ImplOp::ImMul => "mul",
                            ImplOp::ImDiv => "div",
                            ImplOp::ImEq => "eq",
                            ImplOp::ImNeq => "neq",
                            ImplOp::ImDrop => continue,
                        };
                        ops.insert(op_name.to_string(), impl_expr.func.clone());
                    }
                }
                _ => {}
            }
        }
    }

    // --- Main compilation entry ---
    pub fn build_ir(defs: &[Def]) -> (MvmProgram, Vec<HostDef>) {
        let mut cg = MvmCodegen::new();
        cg.collect_struct_info(defs);

        // Pass 1: collect function names and signatures.
        // Track the current module so we can register functions under their
        // fully qualified names (e.g. `mvp_std.atomic.free`).  This avoids
        // collisions when different modules define functions with the same
        // bare name (`free` in both std.atomic and std.mutex).
        let mut current_module: Option<String> = None;
        for def in defs {
            match def {
                Def::DModule { name, .. } => {
                    // Module names like "std.atomic" → qualified prefix "mvp_std.atomic"
                    current_module = Some(if name.starts_with("std") {
                        format!("mvp_{}", name)
                    } else if name.starts_with("main") {
                        format!("main.{}", &name[4..])
                    } else {
                        name.clone()
                    });
                    continue;
                }
                Def::DFunc { name, params, is_async, returns, .. } => {
                    let idx = cg.functions.len();
                    // Compute the qualified function name so calls like
                    // `mvp_std.mutex.free` resolve to the correct slot.
                    let qual_name = current_module.as_ref()
                        .map(|mod_| format!("{}.{}", mod_, name))
                        .unwrap_or_else(|| name.clone());
                    // Register under both the qualified name and the bare name.
                    cg.func_indices.insert(qual_name.clone(), idx);
                    let bare = name.rsplit('.').next().unwrap_or(name);
                    if !cg.func_indices.contains_key(bare) {
                        cg.func_indices.insert(bare.to_string(), idx);
                    }
                    // Placeholder function (will be filled in pass 2)
                    cg.functions.push(MvmFunction {
                        name_idx: 0,
                        arity: params.len() as u32,
                        locals: 0,
                        is_async: *is_async,
                        code: Vec::new(),
                    });
                    // Collect PRef parameter names for reference-parameter
                    // analysis (used by SExpr caller-store-back logic).
                    let ref_param_names: Vec<String> = params
                        .iter()
                        .filter_map(|p| {
                            if let Param::PRef { name, .. } = p {
                                Some(name.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    // Use the qualified function name as the key so that
                    // void_ref_params / func_ref_params lookups inside the
                    // function body use the right entry.
                    cg.func_ref_params.insert(qual_name.clone(), ref_param_names.clone());
                    if returns.is_none() && !ref_param_names.is_empty() {
                        cg.void_ref_params.insert(qual_name.clone(), ref_param_names.clone());
                        let bare = name.rsplit('.').next().unwrap_or(name);
                        if !cg.void_ref_params.contains_key(bare) {
                            cg.void_ref_params.insert(bare.to_string(), ref_param_names);
                        }
                    }
                }
                Def::DTest { name, .. } => {
                    if !cg.func_indices.contains_key(name) {
                        let idx = cg.functions.len();
                        cg.func_indices.insert(name.clone(), idx);
                        cg.functions.push(MvmFunction {
                            name_idx: 0,
                            arity: 0,
                            locals: 0,
                            is_async: false,
                            code: Vec::new(),
                        });
                    }
                }
                Def::DCFuncUnsafe { name, params, returns, code, .. } => {
                    if !cg.host_funcs.contains(name) {
                        cg.host_funcs.insert(name.clone());
                        cg.host_defs.push(HostDef {
                            name: name.clone(),
                            arity: params.len() as u32,
                            returns: returns.clone(),
                            code: code.clone(),
                        });
                    }
                }
                _ => {}
            }
        }

        // Pass 2: compile each function body
        let mut current_module: Option<String> = None;
        for def in defs {
            match def {
                Def::DModule { name, .. } => {
                    current_module = Some(if name.starts_with("std") {
                        format!("mvp_{}", name)
                    } else if name.starts_with("main") {
                        format!("main.{}", &name[4..])
                    } else {
                        name.clone()
                    });
                }
                Def::DFunc { name, params, returns, body, .. } => {
                    let qual_name = current_module.as_ref()
                        .map(|mod_| format!("{}.{}", mod_, name))
                        .unwrap_or_else(|| name.clone());
                    let func_idx = cg.func_indices[&qual_name];
                    cg.current_func_name = qual_name;
                    cg.compile_function(func_idx, params, body, false, returns);
                }
                Def::DTest { name, body, .. } => {
                    let func_idx = cg.func_indices[name];
                    cg.current_func_name = name.clone();
                    cg.compile_function(func_idx, &[], body, true, &None);
                }
                _ => {}
            }
        }

        // Fill in name indices (now that all strings are collected).
        // A function may be registered under both its qualified and bare name;
        // write qualified names first so the bare name wins deterministically
        // (the VM resolves the entry point by looking up the bare "main").
        let mut func_names: Vec<(String, usize)> = cg.func_indices.iter()
            .map(|(name, &idx)| (name.clone(), idx))
            .collect();
        func_names.sort_by_key(|(name, _)| (!name.contains('.'), name.clone()));
        for (func_name, idx) in &func_names {
            let name_idx = cg.resolve_string(func_name);
            cg.functions[*idx].name_idx = name_idx;
        }

        let program = MvmProgram {
            strings: cg.string_pool,
            functions: cg.functions,
        };

        (program, cg.host_defs)
    }

    fn compile_function(&mut self, func_idx: usize, params: &[Param], body: &Expr, _is_test: bool, returns: &Option<Typ>) {
        // Reset state for new function
        self.code = Vec::new();
        self.locals_count = 0;
        self.local_indices.clear();
        self.scope_stack.clear();
        self.labels.clear();
        self.next_label = 0;
        self.param_types.clear();

        // Declare parameters as locals and store their types
        for param in params {
            match param {
                Param::PRef { name, typ, .. } | Param::POwn { name, typ, .. } => {
                    self.declare_local(name);
                    self.param_types.insert(name.clone(), typ.clone());
                    self.var_types.insert(name.clone(), typ.clone());
                }
            }
        }

        // Compile the function body
        // For non-void functions, handle implicit return from EBlock last expression.
        if returns.is_some() {
            if let Expr::EBlock { stmts, result, .. } = body {
                if result.is_none() && !stmts.is_empty() {
                    self.push_scope();
                    // Compile all stmts except the last one normally.
                    for stmt in &stmts[..stmts.len() - 1] {
                        self.compile_stmt(stmt);
                    }
                    // Compile the last stmt as an expression (no trailing Drop).
                    if let Stmt::SExpr { expr, .. } = &stmts[stmts.len() - 1] {
                        self.compile_expr(expr);
                    } else {
                        self.compile_stmt(&stmts[stmts.len() - 1]);
                    }
                    self.pop_scope();
                    if !matches!(self.last_emitted, Some(MvmOp::Ret | MvmOp::RetVal)) {
                        self.emit_op(MvmOp::RetVal);
                    }
                    // IMPORTANT: update function table BEFORE returning, otherwise
                    // the bytecode we just generated is lost (the normal path's
                    // function table update at the end of this method is skipped).
                    let arity = params.len() as u32;
                    self.functions[func_idx].arity = arity;
                    self.functions[func_idx].locals = self.locals_count;
                    self.functions[func_idx].code = std::mem::take(&mut self.code);
                    return; // skip the `compile_expr(body)` and PushUnit below
                }
            }
        }
        self.push_scope();
        self.compile_expr(body);
        self.pop_scope();

        // If the function body didn't end with a return, add one. Use the
        // tracked last-emitted opcode so a multi-byte instruction's operand
        // byte (e.g. CallBuiltin's u8 index) is never mistaken for a terminal
        // Ret/RetVal opcode.
        let needs_ret = !self.code.is_empty()
            && !matches!(self.last_emitted, Some(MvmOp::Ret | MvmOp::RetVal));
        if needs_ret {
            if returns.is_none() {
                let ref_param_names = self.func_ref_params.get(&self.current_func_name);
                if let Some(names) = ref_param_names {
                    if let Some(ref_name) = names.first() {
                        if let Some(idx) = self.resolve_local(ref_name) {
                            self.emit_op(MvmOp::Drop);
                            self.emit_op(MvmOp::LoadLocal);
                            self.emit_u32(idx);
                        }
                    }
                }
            }
            self.emit_op(MvmOp::RetVal);
        }
        self.last_emitted = None;

        // Save this function's code, since compile_thunk reuses `self.code`
        // and takes it for each thunk it emits.
        let mut func_code = std::mem::take(&mut self.code);
        // Save locals count: thunk compilation resets and rebases it.
        let saved_locals = self.locals_count;

        // Flush any lambda thunks registered during this function's body.
        // Nested lambdas may register further thunks, so loop to completion.
        while let Some(thunk) = self.pending_thunks.pop() {
            self.compile_thunk(thunk);
        }

        self.code = func_code;
        self.locals_count = saved_locals;

        // Update function in table
        let arity = params.len() as u32;
        self.functions[func_idx].arity = arity;
        self.functions[func_idx].locals = self.locals_count;
        self.functions[func_idx].code = std::mem::take(&mut self.code);
    }

    /// Compile a lambda thunk into its own function-table slot. Captures become
    /// the leading locals (in `captures` order); the lambda's own parameters
    /// follow. The body is compiled in this fresh scope.
    fn compile_thunk(&mut self, thunk: ClosureThunk) {
        let func_idx = thunk.func_idx;
        let body = thunk.body;
        let ret = thunk.ret;
        // Reset per-function state (mirrors compile_function setup).
        self.code = Vec::new();
        self.locals_count = 0;
        self.local_indices.clear();
        self.scope_stack.clear();
        self.labels.clear();
        self.next_label = 0;
        self.param_types.clear();
        self.var_types.clear();

        // Declare captures as leading locals.
        for (name, typ) in &thunk.captures {
            let idx = self.declare_local(name);
            self.var_types.insert(name.clone(), typ.clone());
            let _ = idx;
        }
        // Declare the lambda's own parameters (after the captures).
        for param in &thunk.params {
            match param {
                Param::PRef { name, typ, .. } | Param::POwn { name, typ, .. } => {
                    self.declare_local(name);
                    self.var_types.insert(name.clone(), typ.clone());
                }
            }
        }

        // Compile the thunk body. If the body is a bare expression (not a
        // block), wrap it so its value is returned.
        let body_expr: Expr = match &body {
            Expr::EBlock { .. } => body.clone(),
            other => Expr::EBlock {
                stmts: Vec::new(),
                result: Some(Box::new(other.clone())),
                loc: crate::ast::Loc { line: 0, col: 0 },
            },
        };

        self.push_scope();
        self.compile_expr(&body_expr);
        self.pop_scope();

        let needs_ret = !self.code.is_empty()
            && !matches!(self.last_emitted, Some(MvmOp::Ret | MvmOp::RetVal));
        if needs_ret {
            self.emit_op(MvmOp::RetVal);
        }
        self.last_emitted = None;

        let total_params = (thunk.captures.len() + thunk.params.len()) as u32;
        self.functions[func_idx].arity = total_params;
        self.functions[func_idx].locals = self.locals_count;
        self.functions[func_idx].code = std::mem::take(&mut self.code);
        let _ = ret;
    }

    // --- Expression compilation ---

    fn compile_expr(&mut self, expr: &Expr) {
        if let Expr::EVar { name, .. } = expr {
            if name == "Shape" {
            }
        }
        match expr {
            Expr::EInt { value, .. } => {
                self.emit_op(MvmOp::PushI64);
                self.emit_i64(*value);
            }
            Expr::EBool { value, .. } => {
                self.emit_op(MvmOp::PushBool);
                self.emit_u8(if *value { 1 } else { 0 });
            }
            Expr::EFloat { value, .. } => {
                // Default to float64 for constant floats
                self.emit_op(MvmOp::PushF64);
                self.emit_f64(*value);
            }
            Expr::EChar { value, .. } => {
                // Char values are stored as strings in Miva JSON but single char
                let c = value.chars().next().unwrap_or('\0') as u8;
                self.emit_op(MvmOp::PushChar);
                self.emit_u8(c);
            }
            Expr::EString { value, .. } => {
                let resolved = crate::codegen::resolve_c_escapes(value);
                let str_idx = self.resolve_string(&resolved);
                self.emit_op(MvmOp::PushString);
                self.emit_u32(str_idx);
            }
            Expr::EVar { name, .. } | Expr::EMove { name, .. } => {
                let idx = self.resolve_local(name)
                    .unwrap_or_else(|| panic!("Variable '{}' not found in scope", name));
                self.emit_op(MvmOp::LoadLocal);
                self.emit_u32(idx);
            }
            Expr::EClone { name, .. } => {
                let idx = self.resolve_local(name)
                    .unwrap_or_else(|| panic!("Variable '{}' not found in scope", name));
                self.emit_op(MvmOp::LoadLocal);
                self.emit_u32(idx);
                self.emit_op(MvmOp::Clone);
            }
            Expr::EStructLit { name, fields, .. } => {
                // Look up struct field definitions to determine field order
                let field_count = self.struct_field_indices.get(name).map(|fl| fl.len()).unwrap_or(0);
                if field_count > 0 {
                    // Emit field values in struct definition order
                    if let Some(field_list) = self.struct_field_indices.get(name) {
                        let mut field_values = vec![None; field_list.len()];
                        for field in fields {
                            if let Some(&(_, idx)) = field_list.iter().find(|(fname, _)| *fname == field.name) {
                                field_values[idx] = Some(&field.value);
                            }
                        }
                        for val in &field_values {
                            if let Some(v) = val {
                                self.compile_expr(v);
                            }
                        }
                    }
                    self.emit_op(MvmOp::StructNew);
                    self.emit_u32(field_count as u32);
                } else {
                    // Unknown struct type; just push unit
                    self.emit_op(MvmOp::PushUnit);
                }
            }
            Expr::EFieldAccess { expr: obj, field, .. } => {
                // Enum discriminant: `Shape.Circle` where `Shape` is an enum type
                // name (not a variable) and `Circle` is a variant. Emit the variant
                // tag directly as a constant for pattern matching.
                if let Expr::EVar { name: enum_name, .. } = obj.as_ref() {
                    if let Some(variant_list) = self.struct_field_indices.get(enum_name) {
                        if let Some(&(_, tag)) = variant_list.iter().find(|(vname, _)| *vname == field.as_str()) {
                            // Emit a unit enum value `Enum(tag, [])` for pattern matching.
                            // EnumNew expects [tag] (no payloads).
                            self.emit_op(MvmOp::PushI64);
                            self.emit_i64(tag as i64);
                            self.emit_op(MvmOp::EnumNew);
                            self.emit_u32(0);
                            return;
                        }
                    }
                }
                if field.chars().all(|c| c.is_ascii_digit()) {
                    let is_tuple = matches!(&obj.as_ref(), Expr::EVar { name, .. } if matches!(self.var_types.get(name), Some(Typ::TTuple { .. })));
                    self.compile_expr(obj);
                    if is_tuple {
                        self.emit_op(MvmOp::StructGet);
                    } else {
                        self.emit_op(MvmOp::EnumGet);
                    }
                    self.emit_u32(field.parse::<u32>().unwrap());
                } else {
                    self.compile_expr(obj);
                    if let Some(field_list) = self.find_field_list(obj) {
                        if let Some(idx) = field_list.iter().position(|(fname, _)| fname == field) {
                            self.emit_op(MvmOp::StructGet);
                            self.emit_u32(idx as u32);
                        } else {
                            // Field not found; emit unit
                            self.emit_op(MvmOp::Drop);
                            self.emit_op(MvmOp::PushUnit);
                        }
                    } else {
                        self.emit_op(MvmOp::Drop);
                        self.emit_op(MvmOp::PushUnit);
                    }
                }
            }
            Expr::EBinOp { op, left, right, .. } => {
                // Check if this operation has an impl override
                if let Some(op_name) = self.find_impl_override(left, op) {
                    // Transform to function call
                    self.compile_expr(left);
                    self.compile_expr(right);
                    let func_idx = *self.func_indices.get(&op_name)
                        .expect(&format!("Impl function '{}' not found", op_name));
                    self.emit_op(MvmOp::Call);
                    self.emit_u32(func_idx as u32);
                } else {
                    self.compile_expr(left);
                    self.compile_expr(right);
                    self.emit_binop(op);
                }
            }
            Expr::EIf { cond, then, else_, .. } => {
                let else_label = self.new_label();
                let end_label = self.new_label();

                self.compile_expr(cond);
                self.emit_jmp_if_false(else_label);
                let before_then = self.code.len();
                self.compile_expr(then);
                let after_then = self.code.len();
                self.emit_jmp(end_label);
                let after_jmp = self.code.len();
                self.define_label(else_label);
                let after_else_label = self.code.len();
                if let Some(else_expr) = else_ {
                    self.compile_expr(else_expr);
                } else {
                    self.emit_op(MvmOp::PushUnit);
                }
                self.define_label(end_label);
            }
            Expr::ECall { name, args, .. } => {
                // Handle module-qualified names: try bare name suffix
                let lookup_name = name.rsplit('.').next().unwrap_or(name);
                // Enum constructor / discriminant: `Name.Variant(args)` where
                // `Name` is a known enum. Compile inline to an EnumNew (or a
                // tag-only discriminant `Enum(tag, [])` for `when (Name.Variant)`).
                if let Some((enum_name, variant)) = name.split_once('.') {
                    if let Some(variant_list) = self.struct_field_indices.get(enum_name) {
                        if let Some(&(_, tag)) = variant_list.iter().find(|(vname, _)| *vname == variant) {
                            self.emit_op(MvmOp::PushI64);
                            self.emit_i64(tag as i64);
                            for arg in args {
                                self.compile_expr(arg);
                            }
                            self.emit_op(MvmOp::EnumNew);
                            self.emit_u32(args.len() as u32);
                            return;
                        }
                    }
                } else if let Some(enum_name) = args.first().and_then(|a| match a {
                    Expr::EVar { name: n, .. } => Some(n.clone()),
                    _ => None,
                }) {
                    // Desugared method-call enum constructor: `Circle(Shape, 5)`
                    // (from `Shape.Circle(5)`) -> EnumNew(5, tag_of_Circle_in_Shape)
                    if let Some(variant_list) = self.struct_field_indices.get(&enum_name) {
                        if let Some(&(_, tag)) = variant_list.iter().find(|(vname, _)| *vname == name.as_str()) {
                            // EnumNew expects: [tag, payload_0, ..., payload_{n-1}] on the
                            // stack (tag pushed first/bottom, payloads on top).
                            self.emit_op(MvmOp::PushI64);
                            self.emit_i64(tag as i64);
                            for arg in &args[1..] {
                                self.compile_expr(arg);
                            }
                            self.emit_op(MvmOp::EnumNew);
                            self.emit_u32((args.len() - 1) as u32);
                            return;
                        }
                    }
                }
                // `await(f)` / `f.await()` unwraps a future (identity for non-futures)
                if lookup_name == "await" {
                    if let Some(arg) = args.first() {
                        self.compile_expr(arg);
                    } else {
                        self.emit_op(MvmOp::PushUnit);
                    }
                    self.emit_op(MvmOp::Await);
                    return;
                }
                // Check if builtin
                if let Some(&builtin_idx) = self.builtin_indices.get(lookup_name) {
                    // Compile args (they'll be on the stack for the builtin)
                    for arg in args {
                        self.compile_expr(arg);
                    }
                    self.emit_op(MvmOp::CallBuiltin);
                    self.emit_u8(builtin_idx);
                } else if self.host_funcs.contains(lookup_name) {
                    // User `unsafe fn` with raw C -> host call into libhost.so.
                    for arg in args {
                        self.compile_expr(arg);
                    }
                    let name_idx = self.resolve_string(lookup_name);
                    self.emit_op(MvmOp::CallHost);
                    self.emit_u32(name_idx);
                    self.emit_u8(args.len() as u8);
                } else if let Some(&func_idx) = self.func_indices.get(name)
                    .or_else(|| {
                        if name.starts_with("std.") {
                            self.func_indices.get(&format!("mvp_{}", name))
                        } else {
                            None
                        }
                    })
                    .or_else(|| self.func_indices.get(lookup_name))
                {
                    for arg in args {
                        self.compile_expr(arg);
                    }
                    self.emit_op(MvmOp::Call);
                    self.emit_u32(func_idx as u32);
                } else if let Some(typ) = self.var_types.get(lookup_name) {
                    if let Typ::TFunc { .. } = typ {
                        // Calling a closure-typed variable: push the arguments,
                        // then load the closure value, then invoke CallClosure.
                        // The VM reads the thunk index and capture count from the
                        // closure value itself.
                        let closure_idx = self.resolve_local(lookup_name)
                            .unwrap_or_else(|| panic!("Closure variable '{}' not found in scope", lookup_name));
                        for arg in args {
                            self.compile_expr(arg);
                        }
                        self.emit_op(MvmOp::LoadLocal);
                        self.emit_u32(closure_idx);
                        self.emit_op(MvmOp::CallClosure);
                    } else {
                        // Unknown function; compile args and push unit
                        for arg in args {
                            self.compile_expr(arg);
                            self.emit_op(MvmOp::Drop);
                        }
                        self.emit_op(MvmOp::PushUnit);
                    }
                } else {
                    // Unknown function; compile args and push unit
                    for arg in args {
                        self.compile_expr(arg);
                        self.emit_op(MvmOp::Drop);
                    }
                    self.emit_op(MvmOp::PushUnit);
                }
            }
            Expr::ECast { expr: inner, to, .. } => {
                self.compile_expr(inner);
                self.emit_cast(to);
            }
            Expr::EBlock { stmts, result, .. } => {
                self.push_scope();
                for stmt in stmts {
                    self.compile_stmt(stmt);
                }
                if let Some(res) = result {
                    self.compile_expr(res);
                } else {
                    self.emit_op(MvmOp::PushUnit);
                }
                self.pop_scope();
            }
            Expr::EArrayLit { values, .. } => {
                for v in values {
                    self.compile_expr(v);
                }
                self.emit_op(MvmOp::PushI64);
                self.emit_i64(values.len() as i64);
                self.emit_op(MvmOp::ArrayNew);
            }
            Expr::ETupleLit { values, .. } => {
                for v in values {
                    self.compile_expr(v);
                }
                self.emit_op(MvmOp::StructNew);
                self.emit_u32(values.len() as u32);
            }
            Expr::EVoid { .. } => {
                self.emit_op(MvmOp::PushUnit);
            }
            Expr::EAddr { expr: inner, .. } => {
                // Take address of a variable
                match inner.as_ref() {
                    Expr::EVar { name, .. } | Expr::EMove { name, .. } => {
                        if let Some(idx) = self.resolve_local(name) {
                            self.emit_op(MvmOp::Addr);
                            self.emit_u32(idx);
                        } else {
                            self.emit_op(MvmOp::PushNull);
                        }
                    }
                    _ => {
                        self.emit_op(MvmOp::PushNull);
                    }
                }
            }
            Expr::EDeref { expr: inner, .. } => {
                self.compile_expr(inner);
                self.emit_op(MvmOp::PtrLoad);
            }
            Expr::EWhile { cond, body, .. } => {
                let cond_label = self.new_label();
                let end_label = self.new_label();

                self.define_label(cond_label);
                self.compile_expr(cond);
                self.emit_jmp_if_false(end_label);
                self.compile_expr(body);
                self.emit_op(MvmOp::Drop); // discard body result
                self.emit_jmp(cond_label);
                self.define_label(end_label);
                self.emit_op(MvmOp::PushUnit);
            }
            Expr::ELoop { body, .. } => {
                let loop_label = self.new_label();
                self.define_label(loop_label);
                self.compile_expr(body);
                self.emit_op(MvmOp::Drop);
                self.emit_jmp(loop_label);
            }
            Expr::EFor { var, range, body, .. } => {
                if !matches!(range.as_ref(), Expr::ECall { name, .. } if name == "range") {
                    self.compile_for_over_array(var, range, body);
                    return;
                }
                // Transform for-loop into a counter-based while loop.
                // We compile the range expression, then create a loop that:
                //  - initializes a counter to 0
                //  - compares it with the range end value at each iteration
                //  - increments the counter
                let check_label = self.new_label();
                let end_label = self.new_label();

                // Allocate a local for the loop counter (if not already)
                let counter_idx = self.declare_local("__for_counter__");
                // Allocate a local for the end value
                let end_idx = self.declare_local("__for_end__");

                // Get the range end value by calling range(n)
                // The range builtin pushes a Range(start, end, current) object
                self.compile_expr(range);
                // Stack: [Range(start, end, current)]
                // We need to extract the end value. For simplicity, call RangeNext
                // which pushes [updated_range, has_next, value]
                // But all we need is the end value. Let's use a simpler approach:
                // range(n) creates Range(0, n, 0) — the value 3 produces end=3
                // But we can't extract the end field from the range easily.
                // Alternative: call range(0, end) and iterate manually.
                // 
                // Actually, simplest approach: iterate using counter
                // The range expression on the stack is a Value::Range
                // We just need to loop n times where n is the only arg
                // 
                // For now, compile the range expression but use a manual counter
                // Drop the range (we don't need it anymore)
                self.emit_op(MvmOp::Drop);

                // Push end value (the range arg was already compiled, so it's
                // on the stack before the range builtin consumed it)
                // BUT: The range builtin consumed the arg! We need to compile
                // the range expression AGAIN to get the arg value.
                // 
                // Better approach: use the arg of range() directly
                // Recompile the range expression's arg
                match range.as_ref() {
                    Expr::ECall { args, .. } if !args.is_empty() => {
                        self.compile_expr(&args[0]);
                    }
                    _ => { self.emit_op(MvmOp::PushI64); self.emit_i64(0); }
                }
                // Stack: [end_value]
                self.emit_op(MvmOp::StoreLocal);
                self.emit_u32(end_idx);
                // Initialize counter = 0
                self.emit_op(MvmOp::PushI64);
                self.emit_i64(0);
                self.emit_op(MvmOp::StoreLocal);
                self.emit_u32(counter_idx);

                // === Loop check ===
                self.define_label(check_label);
                // Load counter
                self.emit_op(MvmOp::LoadLocal);
                self.emit_u32(counter_idx);
                // Load end
                self.emit_op(MvmOp::LoadLocal);
                self.emit_u32(end_idx);
                // Compare: counter < end ?
                self.emit_op(MvmOp::CmpLt);
                // If not less, exit loop
                self.emit_jmp_if_false(end_label);

                // Counter < end, so execute body with current counter value
                // Store counter value as the loop variable
                let var_idx = if let Some(idx) = self.resolve_local(var) {
                    idx
                } else {
                    self.declare_local(var)
                };
                self.emit_op(MvmOp::LoadLocal);
                self.emit_u32(counter_idx);
                self.emit_op(MvmOp::StoreLocal);
                self.emit_u32(var_idx);

                // Compile body
                self.compile_expr(body);
                self.emit_op(MvmOp::Drop); // discard body result

                // Increment counter
                self.emit_op(MvmOp::LoadLocal);
                self.emit_u32(counter_idx);
                self.emit_op(MvmOp::PushI64);
                self.emit_i64(1);
                self.emit_op(MvmOp::I64Add);
                self.emit_op(MvmOp::StoreLocal);
                self.emit_u32(counter_idx);

                // Jump back to check
                self.emit_jmp(check_label);
                self.define_label(end_label);
                self.emit_op(MvmOp::PushUnit);
            }
            Expr::EChoose { var, cases, otherwise, .. } => {
                let end_label = self.new_label();
                let case_labels: Vec<u32> = (0..cases.len()).map(|_| self.new_label()).collect();

                // Original matched value, re-loadable for payload binding.
                let var_clone = var.clone();

                self.compile_expr(var);
                for (i, case) in cases.iter().enumerate() {
                    // Dup the var value for comparison
                    self.emit_op(MvmOp::Dup);
                    if let Expr::EEnumPattern {
                        enum_name,
                        variant,
                        ..
                    } = case.when.as_ref()
                    {
                        // Emit a tag-only enum value for the variant discriminant.
                        let tag = self
                            .struct_field_indices
                            .get(enum_name)
                            .and_then(|vl| vl.iter().find(|(v, _)| v == variant).map(|(_, t)| *t))
                            .unwrap_or(0);
                        self.emit_op(MvmOp::PushI64);
                        self.emit_i64(tag as i64);
                        self.emit_op(MvmOp::EnumNew);
                        self.emit_u32(0);
                    } else {
                        self.compile_expr(&case.when);
                    }
                    self.emit_op(MvmOp::CmpEq);
                    self.emit_jmp_if_false(case_labels[i]);
                    if case.guard.is_some() {
                        // Keep var on stack for guard check; destructure first
                        // so the guard can reference the bound locals.
                        if let Expr::EEnumPattern { bindings, .. } = case.when.as_ref() {
                            for (bi, b) in bindings.iter().enumerate() {
                                self.compile_expr(&var_clone);
                                self.emit_op(MvmOp::EnumGet);
                                self.emit_u32(bi as u32);
                                let bl = self.declare_local(b);
                                self.emit_op(MvmOp::StoreLocal);
                                self.emit_u32(bl);
                            }
                        }
                        let guard = case.guard.as_ref().unwrap();
                        self.compile_expr(guard);
                        self.emit_jmp_if_false(case_labels[i]);
                        self.emit_op(MvmOp::Drop); // drop the var
                        self.compile_expr(&case.then);
                    } else {
                        self.emit_op(MvmOp::Drop); // drop the dup'd var
                        // Destructure payload fields into binding locals.
                        if let Expr::EEnumPattern { bindings, .. } = case.when.as_ref() {
                            for (bi, b) in bindings.iter().enumerate() {
                                self.compile_expr(&var_clone);
                                self.emit_op(MvmOp::EnumGet);
                                self.emit_u32(bi as u32);
                                let bl = self.declare_local(b);
                                self.emit_op(MvmOp::StoreLocal);
                                self.emit_u32(bl);
                            }
                        }
                        self.compile_expr(&case.then);
                    }
                    self.emit_jmp(end_label);
                    self.define_label(case_labels[i]);
                }
                self.emit_op(MvmOp::Drop); // drop the var
                if let Some(other) = otherwise {
                    self.compile_expr(other);
                } else {
                    self.emit_op(MvmOp::PushUnit);
                }
                self.define_label(end_label);
            }
            Expr::EMacro { .. } => {} // Already expanded
            Expr::EMacroVar { .. } => { self.emit_op(MvmOp::PushUnit); }
            Expr::EMethodCall { expr, method, args, .. } => {
                if let Expr::EVar { name: enum_name, .. } = expr.as_ref() {
                    if let Some(variant_list) = self.struct_field_indices.get(enum_name) {
                        if let Some(&(_, tag)) = variant_list.iter().find(|(vname, _)| vname.as_str() == method.as_str()) {
                            self.emit_op(MvmOp::PushI64);
                            self.emit_i64(tag as i64);
                            for arg in args {
                                self.compile_expr(arg);
                            }
                            self.emit_op(MvmOp::EnumNew);
                            self.emit_u32(args.len() as u32);
                            return;
                        }
                    }
                }
                unreachable!("EMethodCall should be desugared by frontend or be an enum constructor");
            }
            Expr::EEnumPattern { .. } => {
                unreachable!("EEnumPattern is handled inline in the EChoose arm")
            }
            Expr::ELambda { params, ret, captures, body, .. } => {
                // Lower a lambda to a closure value. Register a fresh thunk
                // function in the function table (compiled after the enclosing
                // function body), capture the listed variables by value, then
                // build the closure value.
                let thunk_idx = self.functions.len();
                self.functions.push(MvmFunction {
                    name_idx: 0,
                    arity: (captures.len() + params.len()) as u32,
                    locals: 0,
                    is_async: false,
                    code: Vec::new(),
                });
                self.pending_thunks.push(ClosureThunk {
                    captures: captures.clone(),
                    params: params.clone(),
                    body: (**body).clone(),
                    ret: ret.clone(),
                    func_idx: thunk_idx,
                });
                // Evaluate each capture expression (in declaration order) and
                // push its value onto the stack for MakeClosure.
                for (cap_name, _) in captures {
                    self.compile_expr(&Expr::EVar {
                        name: cap_name.clone(),
                        loc: crate::ast::Loc { line: 0, col: 0 },
                    });
                }
                self.emit_op(MvmOp::MakeClosure);
                self.emit_u32(captures.len() as u32);
                self.emit_u32(thunk_idx as u32);
            }
        }
    }

    /// `for (x in arr) { ... }` over an array value: index-based loop using
    /// ArrayLen/ArrayGet, loading each element into the loop variable.
    fn compile_for_over_array(&mut self, var: &str, range: &Expr, body: &Expr) {
        let check_label = self.new_label();
        let end_label = self.new_label();

        let arr_idx = self.declare_local("__for_arr__");
        let counter_idx = self.declare_local("__for_counter__");
        let end_idx = self.declare_local("__for_end__");

        self.compile_expr(range);
        self.emit_op(MvmOp::Dup);
        self.emit_op(MvmOp::StoreLocal);
        self.emit_u32(arr_idx);
        self.emit_op(MvmOp::ArrayLen);
        self.emit_op(MvmOp::StoreLocal);
        self.emit_u32(end_idx);

        self.emit_op(MvmOp::PushI64);
        self.emit_i64(0);
        self.emit_op(MvmOp::StoreLocal);
        self.emit_u32(counter_idx);

        self.define_label(check_label);
        self.emit_op(MvmOp::LoadLocal);
        self.emit_u32(counter_idx);
        self.emit_op(MvmOp::LoadLocal);
        self.emit_u32(end_idx);
        self.emit_op(MvmOp::CmpLt);
        self.emit_jmp_if_false(end_label);

        let var_idx = if let Some(idx) = self.resolve_local(var) {
            idx
        } else {
            self.declare_local(var)
        };
        self.emit_op(MvmOp::LoadLocal);
        self.emit_u32(arr_idx);
        self.emit_op(MvmOp::LoadLocal);
        self.emit_u32(counter_idx);
        self.emit_op(MvmOp::ArrayGet);
        self.emit_op(MvmOp::StoreLocal);
        self.emit_u32(var_idx);

        self.compile_expr(body);
        self.emit_op(MvmOp::Drop);

        self.emit_op(MvmOp::LoadLocal);
        self.emit_u32(counter_idx);
        self.emit_op(MvmOp::PushI64);
        self.emit_i64(1);
        self.emit_op(MvmOp::I64Add);
        self.emit_op(MvmOp::StoreLocal);
        self.emit_u32(counter_idx);

        self.emit_jmp(check_label);
        self.define_label(end_label);
        self.emit_op(MvmOp::PushUnit);
    }

    // --- Statement compilation ---

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::SLetTuple { patterns, expr, .. } => {
                let tuple_idx = self.declare_local("__tuple");
                self.compile_expr(expr);
                self.emit_op(MvmOp::StoreLocal);
                self.emit_u32(tuple_idx);
                for (i, name) in patterns.iter().enumerate() {
                    let idx = self.declare_local(name);
                    self.emit_op(MvmOp::LoadLocal);
                    self.emit_u32(tuple_idx);
                    self.emit_op(MvmOp::StructGet);
                    self.emit_u32(i as u32);
                    self.emit_op(MvmOp::StoreLocal);
                    self.emit_u32(idx);
                }
            }
            Stmt::SLet { name, expr, .. } => {
                let idx = self.declare_local(name);
                // An untyped binding of a lambda is a closure-typed variable.
                if let Expr::ELambda { params, ret, .. } = &**expr {
                    self.var_types.insert(name.clone(), Typ::TFunc {
                        params: params.iter().map(|p| match p {
                            Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ.clone(),
                        }).collect(),
                        returns: Box::new(ret.clone()),
                    });
                }
                self.compile_expr(expr);
                self.emit_op(MvmOp::StoreLocal);
                self.emit_u32(idx);
            }
            Stmt::SLetTyped { name, typ, expr, .. } => {
                let idx = self.declare_local(name);
                self.var_types.insert(name.clone(), typ.clone());
                self.compile_expr(expr);
                self.emit_op(MvmOp::StoreLocal);
                self.emit_u32(idx);
            }
            Stmt::SAssign { name, expr, .. } => {
                if let Some(idx) = self.resolve_local(name) {
                    self.compile_expr(expr);
                    self.emit_op(MvmOp::StoreLocal);
                    self.emit_u32(idx);
                } else {
                    // Variable not found; just evaluate expr and discard
                    self.compile_expr(expr);
                    self.emit_op(MvmOp::Drop);
                }
            }
            Stmt::SFieldAssign { target, field, expr, .. } => {
                // `target.field = expr` — MVM backend field write.
                // Compile the target struct, the new value, emit StructSet
                // with the field index, then store the modified struct back
                // into the target local slot.
                if let Expr::EVar { name, .. } | Expr::EMove { name, .. } = target.as_ref() {
                    if let Some(local_idx) = self.resolve_local(name) {
                        if let Some(field_list) = self.find_field_list(target) {
                            if let Some(idx) = field_list.iter().position(|(fname, _)| fname == field) {
                                self.compile_expr(target);
                                self.compile_expr(expr);
                                self.emit_op(MvmOp::StructSet);
                                self.emit_u32(idx as u32);
                                self.emit_op(MvmOp::StoreLocal);
                                self.emit_u32(local_idx);
                                return;
                            }
                        }
                    }
                }
                // Fallback for unhandled targets: evaluate both and discard.
                self.compile_expr(target);
                self.compile_expr(expr);
                self.emit_op(MvmOp::Drop);
                self.emit_op(MvmOp::Drop);
            }
            Stmt::SReturn { expr, .. } => {
                self.compile_expr(expr);
                if self.current_func_name.is_empty() {
                    self.emit_op(MvmOp::RetVal);
                } else {
                    let is_void_ref = self.void_ref_params.contains_key(&self.current_func_name);
                    if is_void_ref {
                        let ref_param_names = self.func_ref_params.get(&self.current_func_name);
                        if let Some(names) = ref_param_names {
                            if let Some(ref_name) = names.first() {
                                if let Some(idx) = self.resolve_local(ref_name) {
                                    self.emit_op(MvmOp::Drop);
                                    self.emit_op(MvmOp::LoadLocal);
                                    self.emit_u32(idx);
                                }
                            }
                        }
                    }
                    self.emit_op(MvmOp::RetVal);
                }
            }
            Stmt::SExpr { expr, .. } => {
                // If the expression is a call to a function with ref
                // parameters, the called function now returns the modified
                // ref-param value (instead of Unit). Store it back into
                // the corresponding local slot.
                if let Expr::ECall { name, args, .. } = expr.as_ref() {
                    // Try full qualified name first, fall back to bare name.
                    let lookup_name = name.rsplit('.').next().unwrap_or(name);
                    let ref_names = self.void_ref_params.get(name)
                        .or_else(|| {
                            if name.starts_with("std.") {
                                self.void_ref_params.get(&format!("mvp_{}", name))
                            } else {
                                None
                            }
                        })
                        .or_else(|| self.void_ref_params.get(lookup_name));
                    if let Some(ref_names) = ref_names {
                        if let Some(first_ref) = ref_names.first() {
                            if let Some(pos) = ref_names.iter().position(|n| n == first_ref) {
                                if pos < args.len() {
                                    if let Expr::EVar { name, .. } | Expr::EMove { name, .. } = &args[pos] {
                                        if let Some(idx) = self.resolve_local(name) {
                                            self.compile_expr(expr);
                                            self.emit_op(MvmOp::StoreLocal);
                                            self.emit_u32(idx);
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                self.compile_expr(expr);
                self.emit_op(MvmOp::Drop);
            }
            Stmt::SCIntro { .. } => {} // C code integration, skip
            Stmt::SEmpty { .. } => {}
        }
    }

    // --- Helper: emit binary operation ---

    fn emit_binop(&mut self, op: &BinOp) {
        match op {
            BinOp::Add => {
                // For simplicity, emit I64Add; the VM will handle type mismatches
                self.emit_op(MvmOp::I64Add);
            }
            BinOp::Sub => self.emit_op(MvmOp::I64Sub),
            BinOp::Mul => self.emit_op(MvmOp::I64Mul),
            BinOp::Div => self.emit_op(MvmOp::I64Div),
            BinOp::Eq => self.emit_op(MvmOp::CmpEq),
            BinOp::Neq => self.emit_op(MvmOp::CmpNeq),
            BinOp::Lt => self.emit_op(MvmOp::CmpLt),
            BinOp::Gt => self.emit_op(MvmOp::CmpGt),
            BinOp::Le => self.emit_op(MvmOp::CmpLe),
            BinOp::Ge => self.emit_op(MvmOp::CmpGe),
            BinOp::And => self.emit_op(MvmOp::I64And),
            BinOp::Or => self.emit_op(MvmOp::I64Or),
        }
    }

    // --- Helper: emit type cast ---

    fn emit_cast(&mut self, to: &Typ) {
        match to {
            Typ::TInt | Typ::TBool => {
                // no-op for common cases (int is the default)
            }
            Typ::TFloat64 => self.emit_op(MvmOp::I64ToF64),
            Typ::TFloat32 => self.emit_op(MvmOp::I64ToF32),
            Typ::TChar => self.emit_op(MvmOp::I64ToChar),
            _ => {}
        }
    }

    // --- Helper: find struct field list for an expression ---

    fn find_field_list(&self, expr: &Expr) -> Option<&Vec<(String, usize)>> {
        let name = self.find_struct_name(expr)?;
        self.struct_field_indices.get(&name)
    }

    /// Resolve the struct/shape type name of an expression, following
    /// nested field-access chains (e.g. `w.h.f`).
    fn find_struct_name(&self, expr: &Expr) -> Option<String> {
        fn typ_name(typ: &Typ) -> Option<String> {
            match typ {
                Typ::TStruct { name, .. } => Some(name.clone()),
                Typ::TShape { name } => Some(name.clone()),
                _ => None,
            }
        }
        match expr {
            Expr::EVar { name, .. } | Expr::EMove { name, .. } => {
                let result = self
                    .var_types
                    .get(name)
                    .and_then(typ_name)
                    .or_else(|| self.param_types.get(name).and_then(typ_name));
                result
            }
            Expr::EStructLit { name, .. } => Some(name.clone()),
            Expr::EFieldAccess { expr, field, .. } => {
                let inner = self.find_struct_name(expr)?;
                let fields = self.struct_defs.get(&inner)?;
                let fd = fields.iter().find(|f| &f.name == field)?;
                typ_name(&fd.typ)
            }
            _ => None,
        }
    }

    // --- Helper: find impl override for a binop ---

    fn find_impl_override(&self, left: &Expr, op: &BinOp) -> Option<String> {
        let op_name = match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "div",
            BinOp::Eq => "eq",
            BinOp::Neq => "neq",
            BinOp::Lt => "lt",
            BinOp::Gt => "gt",
            BinOp::Le => "le",
            BinOp::Ge => "ge",
            BinOp::And => "and",
            BinOp::Or => "or",
        };

        // Try to find any impl that matches this operator
        for (_struct_name, impls) in &self.impl_map {
            if let Some(func_name) = impls.get(op_name) {
                return Some(func_name.clone());
            }
        }
        None
    }
}

/// Build MVM bytecode for the given AST definitions.
pub fn build_ir(defs: &[Def]) -> (MvmProgram, Vec<HostDef>) {
    MvmCodegen::build_ir(defs)
}
