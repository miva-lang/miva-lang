use crate::ast::*;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

mod analyze;
mod defs;
mod expr;
#[cfg(test)]
mod tests;

pub(crate) use analyze::*;
pub(crate) use defs::*;
pub(crate) use expr::*;

const TARGET_TRIPLE: &str = "x86_64-pc-linux-gnu";

static STR_CONST_COUNTER: AtomicUsize = AtomicUsize::new(0);

static EXTERN_DECLS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Accumulated LLVM IR for closure thunk functions, emitted at module level
/// after all user functions. Reset at the start of each `build_ir` call.
static CLOSURE_THUNK_DEFS: Mutex<Option<String>> = Mutex::new(None);

static CLOSURE_THUNK_ID: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn module_parts(name: &str) -> Vec<String> {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.first() == Some(&"std") || parts.first() == Some(&"mvp_std") {
        let first = if parts.first() == Some(&"std") {
            "mvp_std".to_string()
        } else {
            parts[0].to_string()
        };
        let mut result = vec![first];
        result.extend(parts[1..].iter().map(|s| s.to_string()));
        result
    } else {
        // For user-defined modules, use bare function names (the frontend does not
        // prefix calls with the module name, so definitions must match).
        Vec::new()
    }
}

pub(crate) fn make_global_name(module: Option<&str>, func: &str) -> String {
    match module {
        Some(m) => {
            let prefix = module_parts(m);
            let all: Vec<String> = prefix
                .iter()
                .cloned()
                .chain(std::iter::once(func.to_string()))
                .collect();
            all.join("_")
        }
        None => func.to_string(),
    }
}

pub(crate) fn collect_struct_types(defs: &[Def]) -> Vec<String> {
    let mut types = Vec::new();
    for def in defs {
        match def {
            Def::DStruct { name, fields, .. } => {
                let field_types: Vec<String> = fields
                    .iter()
                    .map(|f| match &f.typ {
                        Typ::TInt => "i64",
                        Typ::TBool | Typ::TChar => "i8",
                        Typ::TFloat64 | Typ::TFloat32 => "double",
                        _ => "i64",
                    })
                    .map(|s| s.to_string())
                    .collect();
                types.push(format!("%{} = type {{ {} }}", name, field_types.join(", ")));
            }
            Def::DShape { name, fields, .. } => {
                let field_types: Vec<String> = fields
                    .iter()
                    .map(|f| match &f.typ {
                        Typ::TInt => "i64",
                        Typ::TBool | Typ::TChar => "i8",
                        Typ::TFloat64 | Typ::TFloat32 => "double",
                        _ => "i64",
                    })
                    .map(|s| s.to_string())
                    .collect();
                types.push(format!("%{} = type {{ {} }}", name, field_types.join(", ")));
            }
            Def::DModule { .. } => {
                types.extend(collect_struct_types(&defs[1..]));
                break;
            }
            _ => {}
        }
    }
    types
}

pub(crate) fn build_struct_field_map(defs: &[Def]) -> HashMap<String, HashMap<String, usize>> {
    let mut map = HashMap::new();
    for def in defs {
        match def {
            Def::DStruct { name, fields, .. } => {
                let field_idx: HashMap<String, usize> = fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| (f.name.clone(), i))
                    .collect();
                map.insert(name.clone(), field_idx);
            }
            Def::DShape { name, fields, .. } => {
                let field_idx: HashMap<String, usize> = fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| (f.name.clone(), i))
                    .collect();
                map.insert(name.clone(), field_idx);
            }
            Def::DModule { .. } => {
                for (k, v) in build_struct_field_map(&defs[1..]) {
                    map.entry(k).or_insert(v);
                }
                break;
            }
            _ => {}
        }
    }
    map
}

pub(crate) fn build_struct_field_types(defs: &[Def]) -> HashMap<String, HashMap<String, Typ>> {
    let mut map = HashMap::new();
    for def in defs {
        match def {
            Def::DStruct { name, fields, .. } => {
                let field_types: HashMap<String, Typ> = fields
                    .iter()
                    .map(|f| (f.name.clone(), f.typ.clone()))
                    .collect();
                map.insert(name.clone(), field_types);
            }
            Def::DShape { name, fields, .. } => {
                let field_types: HashMap<String, Typ> = fields
                    .iter()
                    .map(|f| (f.name.clone(), f.typ.clone()))
                    .collect();
                map.insert(name.clone(), field_types);
            }
            Def::DModule { .. } => {
                for (k, v) in build_struct_field_types(&defs[1..]) {
                    map.entry(k).or_insert(v);
                }
                break;
            }
            _ => {}
        }
    }
    map
}

pub(crate) fn runtime_declarations() -> String {
    let mut decls = String::new();
    decls.push_str("declare void @miva_print(ptr)\n");
    decls.push_str("declare void @miva_println(ptr)\n");
    decls.push_str("declare void @miva_prints(ptr)\n");
    decls.push_str("declare void @miva_printlns(ptr)\n");
    decls.push_str("declare void @miva_error(ptr)\n");
    decls.push_str("declare void @miva_errorln(ptr)\n");
    decls.push_str("declare void @miva_errors(ptr)\n");
    decls.push_str("declare void @miva_errorlns(ptr)\n");
    decls.push_str("declare void @miva_exit(i64)\n");
    decls.push_str("declare void @miva_abort()\n");
    decls.push_str("declare void @miva_panic(ptr)\n");
    decls.push_str("declare ptr @miva_string_concat(ptr, ptr)\n");
    decls.push_str("declare i64 @miva_string_parse(ptr)\n");
    decls.push_str("declare i64 @miva_string_length(ptr)\n");
    decls.push_str("declare ptr @miva_string_make(ptr, i64)\n");
    decls.push_str("declare ptr @miva_string_from_int(i64)\n");
    decls.push_str("declare ptr @miva_string_from_float(double)\n");
    decls.push_str("declare ptr @miva_string_from_bool(i8)\n");
    decls.push_str("declare ptr @miva_string_from_str(ptr)\n");
    decls.push_str("declare ptr @miva_string_c_str(ptr)\n");
    decls.push_str("declare ptr @miva_string_from_cstr(ptr)\n");
    decls.push_str("declare void @miva_box_new_int(ptr, i64)\n");
    decls.push_str("declare void @miva_box_new_float(ptr, double)\n");
    decls.push_str("declare void @miva_box_new_bool(ptr, i8)\n");
    decls.push_str("declare void @miva_box_new_byte(ptr, i8)\n");
    decls.push_str("declare void @miva_box_new_string(ptr, ptr)\n");
    decls.push_str("declare i64 @miva_box_deref_int(ptr)\n");
    decls.push_str("declare double @miva_box_deref_float(ptr)\n");
    decls.push_str("declare i8 @miva_box_deref_bool(ptr)\n");
    decls.push_str("declare i8 @miva_box_deref_byte(ptr)\n");
    decls.push_str("declare void @miva_box_deref_string(ptr, ptr)\n");
    decls.push_str("declare void @miva_range(ptr, i64, i64)\n");
    decls.push_str("declare void @miva_range_end(ptr, i64)\n");
    decls.push_str("declare void @miva_range_step(ptr, i64, i64, i64)\n");
    decls.push_str("declare ptr @miva_alloc(i64)\n");
    decls.push_str("declare ptr @miva_realloc(ptr, i64)\n");
    decls.push_str("declare void @miva_free(ptr)\n");
    decls.push_str("declare ptr @miva_ptr_offset(ptr, i64)\n");
    decls.push_str("declare void @miva_ptr_set_i64(ptr, i64)\n");
    decls.push_str("declare void @miva_ptr_set_double(ptr, double)\n");
    decls.push_str("declare void @miva_ptr_set_i8(ptr, i8)\n");
    decls.push_str("declare void @miva_ptr_set_ptr(ptr, ptr)\n");
    decls.push_str("declare i64 @miva_async_await(i64)\n");
    decls.push_str("declare i64 @miva_async_spawn(ptr, i64)\n");
    decls.push_str("declare ptr @miva_mutex_new()\n");
    decls.push_str("declare void @miva_mutex_lock(i64)\n");
    decls.push_str("declare void @miva_mutex_unlock(i64)\n");
    decls.push_str("declare void @miva_mutex_free(i64)\n");
    decls.push_str("declare i64 @miva_json_parse(ptr)\n");
    decls.push_str("declare i64 @miva_json_kind(i64)\n");
    decls.push_str("declare i64 @miva_json_bool(i64)\n");
    decls.push_str("declare i64 @miva_json_number(i64)\n");
    decls.push_str("declare ptr @miva_json_string(i64)\n");
    decls.push_str("declare i64 @miva_json_array_len(i64)\n");
    decls.push_str("declare i64 @miva_json_array_get(i64, i64)\n");
    decls.push_str("declare i64 @miva_json_object_len(i64)\n");
    decls.push_str("declare ptr @miva_json_object_key(i64, i64)\n");
    decls.push_str("declare i64 @miva_json_object_get(i64, i64)\n");
    decls.push_str("declare i64 @miva_json_object_find(i64, ptr)\n");
    decls.push_str("declare void @miva_json_free(i64)\n");
    decls.push_str("declare ptr @miva_json_stringify(i64)\n");
    decls.push_str("declare i64 @miva_xml_parse(ptr)\n");
    decls.push_str("declare i64 @miva_xml_kind(i64)\n");
    decls.push_str("declare ptr @miva_xml_tag(i64)\n");
    decls.push_str("declare i64 @miva_xml_attr_count(i64)\n");
    decls.push_str("declare ptr @miva_xml_attr_name(i64, i64)\n");
    decls.push_str("declare ptr @miva_xml_attr_value(i64, i64)\n");
    decls.push_str("declare ptr @miva_xml_attr_find(i64, ptr)\n");
    decls.push_str("declare i64 @miva_xml_child_count(i64)\n");
    decls.push_str("declare i64 @miva_xml_child_get(i64, i64)\n");
    decls.push_str("declare ptr @miva_xml_text(i64)\n");
    decls.push_str("declare ptr @miva_xml_comment(i64)\n");
    decls.push_str("declare ptr @miva_xml_cdata(i64)\n");
    decls.push_str("declare ptr @miva_xml_pi_target(i64)\n");
    decls.push_str("declare ptr @miva_xml_pi_data(i64)\n");
    decls.push_str("declare ptr @miva_xml_stringify(i64)\n");
    decls.push_str("declare void @miva_xml_free(i64)\n");
    decls.push_str("declare i64 @miva_toml_parse(ptr)\n");
    decls.push_str("declare i64 @miva_toml_kind(i64)\n");
    decls.push_str("declare i64 @miva_toml_bool(i64)\n");
    decls.push_str("declare i64 @miva_toml_number(i64)\n");
    decls.push_str("declare ptr @miva_toml_string(i64)\n");
    decls.push_str("declare i64 @miva_toml_array_len(i64)\n");
    decls.push_str("declare i64 @miva_toml_array_get(i64, i64)\n");
    decls.push_str("declare i64 @miva_toml_object_len(i64)\n");
    decls.push_str("declare ptr @miva_toml_object_key(i64, i64)\n");
    decls.push_str("declare i64 @miva_toml_object_get(i64, i64)\n");
    decls.push_str("declare i64 @miva_toml_object_find(i64, ptr)\n");
    decls.push_str("declare void @miva_toml_free(i64)\n");
    decls.push_str("declare ptr @miva_toml_stringify(i64)\n");
    decls.push_str("declare i64 @miva_yaml_parse(ptr)\n");
    decls.push_str("declare i64 @miva_yaml_kind(i64)\n");
    decls.push_str("declare i64 @miva_yaml_bool(i64)\n");
    decls.push_str("declare i64 @miva_yaml_number(i64)\n");
    decls.push_str("declare ptr @miva_yaml_string(i64)\n");
    decls.push_str("declare i64 @miva_yaml_array_len(i64)\n");
    decls.push_str("declare i64 @miva_yaml_array_get(i64, i64)\n");
    decls.push_str("declare i64 @miva_yaml_object_len(i64)\n");
    decls.push_str("declare ptr @miva_yaml_object_key(i64, i64)\n");
    decls.push_str("declare i64 @miva_yaml_object_get(i64, i64)\n");
    decls.push_str("declare i64 @miva_yaml_object_find(i64, ptr)\n");
    decls.push_str("declare void @miva_yaml_free(i64)\n");
    decls.push_str("declare ptr @miva_yaml_stringify(i64)\n");
    decls.push_str("@.str.void = private unnamed_addr constant [1 x i8] zeroinitializer\n");
    decls
}

pub(crate) fn map_builtin(name: &str, current_module: Option<&str>) -> String {
    match name {
        "print" => "@miva_print".into(),
        "prints" => "@miva_prints".into(),
        "println" => "@miva_println".into(),
        "printlns" => "@miva_printlns".into(),
        "error" => "@miva_error".into(),
        "errors" => "@miva_errors".into(),
        "errorln" => "@miva_errorln".into(),
        "errorlns" => "@miva_errorlns".into(),
        "exit" => "@miva_exit".into(),
        "abort" => "@miva_abort".into(),
        "panic" => "@miva_panic".into(),
        "string_concat" => "@miva_string_concat".into(),
        "string_parse" => "@miva_string_parse".into(),
        "string_length" => "@miva_string_length".into(),
        "string_make" => "@miva_string_make".into(),
        "string_from" => "@miva_string_from_int".into(),
        "box_new" => "@miva_box_new_int".into(),
        "box_deref" => "@miva_box_deref_int".into(),
        "range" => "@miva_range".into(),
        "ptr_alloc" => "@miva_alloc".into(),
        "ptr_realloc" => "@miva_realloc".into(),
        "ptr_free" => "@miva_free".into(),
        "ptr_set" => "@miva_ptr_set_i64".into(),
        "ptr_offset" => "@miva_ptr_offset".into(),
        "await" => "@miva_async_await".into(),
        "json_parse" => "@miva_json_parse".into(),
        "json_kind" => "@miva_json_kind".into(),
        "json_bool" => "@miva_json_bool".into(),
        "json_number" => "@miva_json_number".into(),
        "json_string" => "@miva_json_string".into(),
        "json_array_len" => "@miva_json_array_len".into(),
        "json_array_get" => "@miva_json_array_get".into(),
        "json_object_len" => "@miva_json_object_len".into(),
        "json_object_key" => "@miva_json_object_key".into(),
        "json_object_get" => "@miva_json_object_get".into(),
        "json_object_find" => "@miva_json_object_find".into(),
        "json_free" => "@miva_json_free".into(),
        "json_stringify" => "@miva_json_stringify".into(),
        "mutex_new" => "@miva_mutex_new".into(),
        "mutex_lock" => "@miva_mutex_lock".into(),
        "mutex_unlock" => "@miva_mutex_unlock".into(),
        "mutex_free" => "@miva_mutex_free".into(),
        "xml_parse" => "@miva_xml_parse".into(),
        "xml_kind" => "@miva_xml_kind".into(),
        "xml_tag" => "@miva_xml_tag".into(),
        "xml_attr_count" => "@miva_xml_attr_count".into(),
        "xml_attr_name" => "@miva_xml_attr_name".into(),
        "xml_attr_value" => "@miva_xml_attr_value".into(),
        "xml_attr_find" => "@miva_xml_attr_find".into(),
        "xml_child_count" => "@miva_xml_child_count".into(),
        "xml_child_get" => "@miva_xml_child_get".into(),
        "xml_text" => "@miva_xml_text".into(),
        "xml_comment" => "@miva_xml_comment".into(),
        "xml_cdata" => "@miva_xml_cdata".into(),
        "xml_pi_target" => "@miva_xml_pi_target".into(),
        "xml_pi_data" => "@miva_xml_pi_data".into(),
        "xml_stringify" => "@miva_xml_stringify".into(),
        "xml_free" => "@miva_xml_free".into(),
        "toml_parse" => "@miva_toml_parse".into(),
        "toml_kind" => "@miva_toml_kind".into(),
        "toml_bool" => "@miva_toml_bool".into(),
        "toml_number" => "@miva_toml_number".into(),
        "toml_string" => "@miva_toml_string".into(),
        "toml_array_len" => "@miva_toml_array_len".into(),
        "toml_array_get" => "@miva_toml_array_get".into(),
        "toml_object_len" => "@miva_toml_object_len".into(),
        "toml_object_key" => "@miva_toml_object_key".into(),
        "toml_object_get" => "@miva_toml_object_get".into(),
        "toml_object_find" => "@miva_toml_object_find".into(),
        "toml_free" => "@miva_toml_free".into(),
        "toml_stringify" => "@miva_toml_stringify".into(),
        "yaml_parse" => "@miva_yaml_parse".into(),
        "yaml_kind" => "@miva_yaml_kind".into(),
        "yaml_bool" => "@miva_yaml_bool".into(),
        "yaml_number" => "@miva_yaml_number".into(),
        "yaml_string" => "@miva_yaml_string".into(),
        "yaml_array_len" => "@miva_yaml_array_len".into(),
        "yaml_array_get" => "@miva_yaml_array_get".into(),
        "yaml_object_len" => "@miva_yaml_object_len".into(),
        "yaml_object_key" => "@miva_yaml_object_key".into(),
        "yaml_object_get" => "@miva_yaml_object_get".into(),
        "yaml_object_find" => "@miva_yaml_object_find".into(),
        "yaml_free" => "@miva_yaml_free".into(),
        "yaml_stringify" => "@miva_yaml_stringify".into(),
        _ => {
            let parts: Vec<&str> = name.split('.').collect();
            if parts.first() == Some(&"ffi") {
                format!("@{}", parts[1..].join("_"))
            } else if parts.len() == 1 {
                let full = match current_module {
                    Some(m) => {
                        let mp = module_parts(m);
                        let all: Vec<String> = mp
                            .iter()
                            .cloned()
                            .chain(std::iter::once(name.to_string()))
                            .collect();
                        all.join("_")
                    }
                    None => name.to_string(),
                };
                format!("@{}", full)
            } else {
                let module = parts[..parts.len() - 1].join(".");
                let func = parts[parts.len() - 1];
                format!("@{}", make_global_name(Some(&module), func))
            }
        }
    }
}

pub(crate) struct LlvmCtx {
    indent: usize,
    tmp_counter: usize,
    string_constants: String,
    current_module: Option<String>,
    var_seq: HashMap<String, usize>,
    var_addrs: HashMap<String, String>,
    var_reloads: HashMap<String, String>,
    struct_field_map: HashMap<String, HashMap<String, usize>>,
    struct_field_types: HashMap<String, HashMap<String, Typ>>,
    field_idx: HashMap<String, usize>,
    func_sigs: HashMap<String, crate::codegen::FuncSig>,
    string_regs: HashSet<String>,
    enum_defs: HashMap<String, (Vec<String>, HashMap<String, Vec<Typ>>)>,
    string_payloads: HashMap<String, Vec<usize>>,
    pending_string_payloads: Vec<usize>,
    var_types: HashMap<String, Typ>,
}

impl LlvmCtx {
    fn new() -> Self {
        LlvmCtx {
            indent: 0,
            tmp_counter: 0,
            string_constants: String::new(),
            current_module: None,
            var_seq: HashMap::new(),
            var_addrs: HashMap::new(),
            var_reloads: HashMap::new(),
            struct_field_map: HashMap::new(),
            struct_field_types: HashMap::new(),
            field_idx: HashMap::new(),
            func_sigs: HashMap::new(),
            string_regs: HashSet::new(),
            enum_defs: HashMap::new(),
            string_payloads: HashMap::new(),
            pending_string_payloads: Vec::new(),
            var_types: HashMap::new(),
        }
    }

    fn with_func_sigs(mut self, sigs: &HashMap<String, crate::codegen::FuncSig>) -> Self {
        self.func_sigs = sigs.clone();
        self
    }

    fn with_module(module: Option<&str>) -> Self {
        LlvmCtx {
            indent: 0,
            tmp_counter: 0,
            string_constants: String::new(),
            current_module: module.map(|m| m.to_string()),
            var_seq: HashMap::new(),
            var_addrs: HashMap::new(),
            var_reloads: HashMap::new(),
            struct_field_map: HashMap::new(),
            struct_field_types: HashMap::new(),
            field_idx: HashMap::new(),
            func_sigs: HashMap::new(),
            string_regs: HashSet::new(),
            enum_defs: HashMap::new(),
            string_payloads: HashMap::new(),
            pending_string_payloads: Vec::new(),
            var_types: HashMap::new(),
        }
    }

    fn with_module_and_fields(
        module: Option<&str>,
        struct_field_map: HashMap<String, HashMap<String, usize>>,
        struct_field_types: HashMap<String, HashMap<String, Typ>>,
        enum_defs: HashMap<String, (Vec<String>, HashMap<String, Vec<Typ>>)>,
    ) -> Self {
        let mut field_idx = HashMap::new();
        for (_sname, fields) in &struct_field_map {
            for (fname, fidx) in fields {
                field_idx.entry(fname.clone()).or_insert(*fidx);
            }
        }
        LlvmCtx {
            indent: 0,
            tmp_counter: 0,
            string_constants: String::new(),
            current_module: module.map(|m| m.to_string()),
            var_seq: HashMap::new(),
            var_addrs: HashMap::new(),
            var_reloads: HashMap::new(),
            struct_field_map,
            struct_field_types,
            field_idx,
            func_sigs: HashMap::new(),
            string_regs: HashSet::new(),
            enum_defs,
            string_payloads: HashMap::new(),
            pending_string_payloads: Vec::new(),
            var_types: HashMap::new(),
        }
    }

    fn gen_tmp(&mut self, prefix: &str) -> String {
        let id = self.tmp_counter;
        self.tmp_counter += 1;
        format!("%{}_{}", prefix, id)
    }

    fn gen_label(&mut self, prefix: &str) -> String {
        let id = self.tmp_counter;
        self.tmp_counter += 1;
        format!("{}_{}", prefix, id)
    }

    fn indent_str(&self) -> String {
        "  ".repeat(self.indent)
    }

    /// Reserve a fresh alloca + initial reload name for `name`. Every call
    /// (SLet, for-loop iteration, etc.) gets a unique `(addr, reload)` pair
    /// driven by the per-variable `var_seq` counter so re-declarations of the
    /// same name in the same function never collide with each other or with
    /// reloads produced by SAssign / emit_fresh_loads (which all live in the
    /// same `s.r.N` namespace).
    fn declare_var(&mut self, name: &str) -> (String, String) {
        let count = self.var_seq.entry(name.to_string()).or_insert(0);
        let addr = format!("{}.addr.{}", name, count);
        let reload = format!("{}.r.{}", name, count);
        self.var_addrs.insert(name.to_string(), addr.clone());
        self.var_reloads.insert(name.to_string(), reload.clone());
        *count += 1;
        (addr, reload)
    }

    fn get_var_addr(&self, name: &str) -> String {
        self.var_addrs
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("get_var_addr: variable '{}' not declared", name))
    }

    fn get_var_reload(&self, name: &str) -> String {
        self.var_reloads
            .get(name)
            .map(|n| format!("%{}", n))
            .unwrap_or_else(|| panic!("get_var_reload: variable '{}' not declared", name))
    }
}

/// Check if an expression likely evaluates to a string value.
pub fn build_ir(
    defs: &[Def],
    func_sigs: &HashMap<String, crate::codegen::FuncSig>,
) -> crate::codegen::GeneratedOutput {
    *EXTERN_DECLS.lock().unwrap() = Some(HashSet::new());
    *CLOSURE_THUNK_DEFS.lock().unwrap() = Some(String::new());
    CLOSURE_THUNK_ID.store(0, Ordering::Relaxed);
    let struct_types = collect_struct_types(defs);
    let struct_field_map = build_struct_field_map(defs);
    let struct_field_types = build_struct_field_types(defs);
    let (struct_defs, defs_str, main_functions, defined) = generate_with_scope(
        defs,
        None,
        &struct_field_map,
        &struct_field_types,
        func_sigs,
    );

    // Collect user DCFuncUnsafe definitions for libhost.c generation
    let mut host_defs: Vec<crate::codegen::mvm::HostDef> = Vec::new();
    for def in defs {
        if let Def::DCFuncUnsafe {
            name,
            params,
            returns,
            code,
            ..
        } = def
        {
            if !host_defs
                .iter()
                .any(|h: &crate::codegen::mvm::HostDef| h.name == *name)
            {
                host_defs.push(crate::codegen::mvm::HostDef {
                    name: name.clone(),
                    arity: params.len() as u32,
                    returns: returns.clone(),
                    code: code.clone(),
                });
            }
        }
    }

    let mut program = String::new();
    program.push_str("; ModuleID = 'miva_output'\n");
    program.push_str(&format!("target triple = \"{}\"\n\n", TARGET_TRIPLE));
    STR_CONST_COUNTER.store(0, Ordering::Relaxed);
    program.push_str("%mvp_builtin_string = type opaque\n");
    program.push_str("%mvp_builtin_box = type opaque\n");
    program.push_str("%MivaValue = type { i64, i64 }\n\n");

    for st in &struct_types {
        program.push_str(&st);
        program.push_str("\n");
    }

    program.push_str(&runtime_declarations());
    if let Ok(guard) = EXTERN_DECLS.lock() {
        if let Some(decls) = guard.as_ref() {
            for decl in decls.iter() {
                let name = decl
                    .trim_start_matches("declare i64 @")
                    .split('(')
                    .next()
                    .unwrap_or("");
                if !defined.contains(name) {
                    program.push_str(&format!("{}\n", decl));
                }
            }
        }
    }

    program.push_str(&struct_defs);
    program.push_str(&defs_str);
    program.push_str(&main_functions);
    if let Ok(guard) = CLOSURE_THUNK_DEFS.lock() {
        if let Some(thunks) = guard.as_ref() {
            program.push_str(thunks);
        }
    }

    // Declare miva_host_* functions (from libhost.c) for inline unsafe functions
    for hd in &host_defs {
        program.push_str(&format!(
            "declare %MivaValue @miva_host_{}(ptr, i32)\n",
            hd.name
        ));
    }

    let test_ir = generate_test(defs);
    let bridge = generate_bridge(defs);

    crate::codegen::GeneratedOutput {
        program: program.into_bytes(),
        header: bridge,
        test: test_ir,
        extension: "ll",
        host_defs,
    }
}
