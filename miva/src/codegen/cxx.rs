//! Shared C++ emission helpers used by the IR-based emitter (`cxx_ir`).

use crate::ast::*;

/// Escape a string value for use in a C++ string literal.
pub fn cxx_escape_string(s: &str) -> String {
    use crate::codegen::resolve_c_escapes;
    let resolved = resolve_c_escapes(s);
    let mut out = String::with_capacity(resolved.len());
    for c in resolved.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '"' => out.push_str("\\\""),
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn indent_str(n: usize) -> String {
    " ".repeat(n * 2)
}

pub fn cxx_type(typ: &Typ) -> String {
    match typ {
        Typ::TInt => "mvp_builtin_int".into(),
        Typ::TBool => "mvp_builtin_boolean".into(),
        Typ::TFloat64 | Typ::TFloat32 => "mvp_builtin_float".into(),
        Typ::TChar => "mvp_builtin_byte".into(),
        Typ::TString => "mvp_builtin_string".into(),
        Typ::TArray { of } => format!("std::vector<{}>", cxx_type(of)),
        Typ::TStruct {
            name, type_args, ..
        } => {
            // Module-qualified type names are emitted by the frontend already
            // joined with `::` (e.g. `std::result::Result`). The leading `std`
            // module must be rewritten to its C++ namespace `mvp_std` so the
            // reference resolves to the imported header. Local types
            // (`Shape`, `Vec`) have no `std::` prefix and pass through.
            let ns_name = if let Some(rest) = name.strip_prefix("std::") {
                format!("mvp_std::{}", rest)
            } else {
                name.clone()
            };
            if type_args.is_empty() {
                ns_name
            } else {
                let args_str = type_args
                    .iter()
                    .map(cxx_type)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}<{}>", ns_name, args_str)
            }
        }
        Typ::TPtr { to } => format!("{}*", cxx_type(to)),
        Typ::TBox { of } => format!("mvp_builtin_box<{}>", cxx_type(of)),
        Typ::TFuture { of } => format!("mvp_future<{}>", cxx_type(of)),
        Typ::TNull => "void".into(),
        Typ::TPtrAny => "mvp_builtin_ptrany".into(),
        Typ::TInvalid => "invalid".into(),
        Typ::TGenericParam { name } => name.clone(),
        Typ::TFunc { params, returns } => {
            let ps: Vec<String> = params.iter().map(|p| cxx_type(p)).collect();
            let r = cxx_type(returns);
            if ps.is_empty() {
                format!("mvp_closure<{}>", r)
            } else {
                format!("mvp_closure<{}, {}>", r, ps.join(", "))
            }
        }
        Typ::TTuple { elems } => {
            format!("std::tuple<{}>", elems.iter().map(cxx_type).collect::<Vec<_>>().join(", "))
        }
        // TShape is a compile-time-only type; it should be erased before codegen.
        Typ::TShape { name } => name.clone(),
    }
}

pub(crate) fn cxx_param(param: &Param) -> String {
    match param {
        Param::PRef { name, typ } => format!("{} const& {}", cxx_type(typ), mangle_cpp_kw(name)),
        Param::POwn { name, typ } => format!("{} {}", cxx_type(typ), mangle_cpp_kw(name)),
    }
}

pub(crate) fn cxx_func_decl(name: &str, params: &[Param], ret: &Option<Typ>) -> String {
    let param_list: Vec<_> = params.iter().map(cxx_param).collect();
    let ret_type = ret.as_ref().map_or("mvp_builtin_unit".into(), cxx_type);
    // Collect any generic type parameters referenced in param/return types
    // (e.g. `check` has no explicit type_params but its param is `Atomic<T>`).
    let mut seen = std::collections::HashSet::new();
    let mut extra_params: Vec<String> = Vec::new();
    for p in params {
        let typ = match p {
            Param::PRef { typ, .. } | Param::POwn { typ, .. } => typ,
        };
        collect_generic_params(typ, &mut seen, &mut extra_params);
    }
    if let Some(r) = ret {
        collect_generic_params(r, &mut seen, &mut extra_params);
    }
    let template = if extra_params.is_empty() {
        String::new()
    } else {
        let params_str = extra_params
            .iter()
            .map(|tp| format!("typename {}", tp))
            .collect::<Vec<_>>()
            .join(", ");
        format!("template<{}>\n", params_str)
    };
    format!("{}{} {}({});\n", template, ret_type, mangle_cpp_kw(name), param_list.join(", "))
}

/// Recursively collect `TGenericParam` names from a type.
pub(crate) fn collect_generic_params(typ: &Typ, seen: &mut std::collections::HashSet<String>, out: &mut Vec<String>) {
    match typ {
        Typ::TGenericParam { name } => {
            if seen.insert(name.clone()) {
                out.push(name.clone());
            }
        }
        Typ::TStruct { name, fields, type_args } => {
            if fields.is_empty() && type_args.is_empty() && name.len() == 1 && name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                if seen.insert(name.clone()) {
                    out.push(name.clone());
                }
            } else {
                for a in type_args {
                    collect_generic_params(a, seen, out);
                }
            }
        }
        Typ::TPtr { to } => collect_generic_params(to, seen, out),
        Typ::TBox { of } => collect_generic_params(of, seen, out),
        Typ::TFuture { of } => collect_generic_params(of, seen, out),
        Typ::TArray { of } => collect_generic_params(of, seen, out),
        Typ::TFunc { params, returns } => {
            for p in params {
                collect_generic_params(p, seen, out);
            }
            collect_generic_params(returns, seen, out);
        }
        Typ::TTuple { elems } => {
            for e in elems {
                collect_generic_params(e, seen, out);
            }
        }
        _ => {}
    }
}

/// Mangle a Miva identifier that collides with a C++ keyword so the emitted
/// C++ compiles. `new`, `delete`, `class`, `template`, `typename`, etc. get an
/// `mvp_` prefix. Applied to every function/definition name we emit and every
/// call name we reference, so caller and callee stay in sync.
pub(crate) fn mangle_cpp_kw(name: &str) -> String {
    // Only mangle the bare (last) identifier — qualified names like
    // `mvp_std::mem::offset` have namespace separators we must preserve.
    // Split on '::' and mangle only the trailing segment.
    let last_seg_start = name.rfind("::").map(|i| i + 2).unwrap_or(0);
    let prefix = &name[..last_seg_start];
    let tail = &name[last_seg_start..];
    let mangled_tail = match tail {
        "new" | "delete" | "class" | "template" | "typename" | "operator"
        | "public" | "private" | "protected" | "virtual" | "namespace"
        | "using" | "struct" | "enum" | "union" | "typedef" | "auto"
        | "static" | "extern" | "const" | "volatile" | "register" | "inline"
        | "friend" | "this" | "throw" | "try" | "catch" | "goto" | "return"
        | "break" | "continue" | "if" | "else" | "while" | "for" | "switch"
        | "case" | "default" | "do" | "sizeof" | "and" | "or" | "not"
        | "xor" | "bitand" | "bitor" | "compl" | "true" | "false" => {
            format!("mvp_{}", tail)
        }
        _ => tail.to_string(),
    };
    format!("{}{}", prefix, mangled_tail)
}

pub fn map_builtin(name: &str) -> String {
    match name {
        "print" => "mvp_print".into(),
        "prints" => "mvp_prints".into(),
        "println" => "mvp_println".into(),
        "printlns" => "mvp_printlns".into(),
        "error" => "mvp_error".into(),
        "errors" => "mvp_errors".into(),
        "errorln" => "mvp_errorln".into(),
        "errorlns" => "mvp_errorlns".into(),
        "exit" => "mvp_exit".into(),
        "abort" => "mvp_abort".into(),
        "panic" => "mvp_panic".into(),
        "string_concat" => "mvp_string_concat".into(),
        "string_parse" => "mvp_string_parse".into(),
        "string_length" => "mvp_string_length".into(),
        "string_make" => "mvp_string_make".into(),
        "string_from" => "mvp_to_string".into(),
        "box_new" => "mvp_box_new".into(),
        "box_deref" => "mvp_box_deref".into(),
        "range" => "mvp_range".into(),
        "ptr_alloc" => "mvp_alloc".into(),
        "ptr_realloc" => "mvp_realloc".into(),
        "ptr_free" => "mvp_free".into(),
        "ptr_set" => "mvp_builtin_ptrset".into(),
        "ptr_offset" => "mvp_ptr_offset".into(),
        "await" => "mvp_async_await".into(),
        "json_parse" => "mvp_json_parse".into(),
        "json_kind" => "mvp_json_kind".into(),
        "json_bool" => "mvp_json_bool".into(),
        "json_number" => "mvp_json_number".into(),
        "json_string" => "mvp_json_string".into(),
        "json_array_len" => "mvp_json_array_len".into(),
        "json_array_get" => "mvp_json_array_get".into(),
        "json_object_len" => "mvp_json_object_len".into(),
        "json_object_key" => "mvp_json_object_key".into(),
        "json_object_get" => "mvp_json_object_get".into(),
        "json_object_find" => "mvp_json_object_find".into(),
        "json_free" => "mvp_json_free".into(),
        "json_stringify" => "mvp_json_stringify".into(),
        "xml_parse" => "mvp_xml_parse".into(),
        "xml_kind" => "mvp_xml_kind".into(),
        "xml_tag" => "mvp_xml_tag".into(),
        "xml_attr_count" => "mvp_xml_attr_count".into(),
        "xml_attr_name" => "mvp_xml_attr_name".into(),
        "xml_attr_value" => "mvp_xml_attr_value".into(),
        "xml_attr_find" => "mvp_xml_attr_find".into(),
        "xml_child_count" => "mvp_xml_child_count".into(),
        "xml_child_get" => "mvp_xml_child_get".into(),
        "xml_text" => "mvp_xml_text".into(),
        "xml_comment" => "mvp_xml_comment".into(),
        "xml_cdata" => "mvp_xml_cdata".into(),
        "xml_pi_target" => "mvp_xml_pi_target".into(),
        "xml_pi_data" => "mvp_xml_pi_data".into(),
        "xml_stringify" => "mvp_xml_stringify".into(),
        "xml_free" => "mvp_xml_free".into(),
        "toml_parse" => "mvp_toml_parse".into(),
        "toml_kind" => "mvp_toml_kind".into(),
        "toml_bool" => "mvp_toml_bool".into(),
        "toml_number" => "mvp_toml_number".into(),
        "toml_string" => "mvp_toml_string".into(),
        "toml_array_len" => "mvp_toml_array_len".into(),
        "toml_array_get" => "mvp_toml_array_get".into(),
        "toml_object_len" => "mvp_toml_object_len".into(),
        "toml_object_key" => "mvp_toml_object_key".into(),
        "toml_object_get" => "mvp_toml_object_get".into(),
        "toml_object_find" => "mvp_toml_object_find".into(),
        "toml_free" => "mvp_toml_free".into(),
        "toml_stringify" => "mvp_toml_stringify".into(),
        "yaml_parse" => "mvp_yaml_parse".into(),
        "yaml_kind" => "mvp_yaml_kind".into(),
        "yaml_bool" => "mvp_yaml_bool".into(),
        "yaml_number" => "mvp_yaml_number".into(),
        "yaml_string" => "mvp_yaml_string".into(),
        "yaml_array_len" => "mvp_yaml_array_len".into(),
        "yaml_array_get" => "mvp_yaml_array_get".into(),
        "yaml_object_len" => "mvp_yaml_object_len".into(),
        "yaml_object_key" => "mvp_yaml_object_key".into(),
        "yaml_object_get" => "mvp_yaml_object_get".into(),
        "yaml_object_find" => "mvp_yaml_object_find".into(),
        "yaml_free" => "mvp_yaml_free".into(),
        "yaml_stringify" => "mvp_yaml_stringify".into(),
        "mutex_new" => "mvp_mutex_new".into(),
        "mutex_lock" => "mvp_mutex_lock".into(),
        "mutex_unlock" => "mvp_mutex_unlock".into(),
        "mutex_free" => "mvp_mutex_free".into(),
        _ => {
            let parts: Vec<&str> = name.split('.').collect();
            if parts.first() == Some(&"ffi") {
                parts[1..].join("::")
            } else {
                // qualified name like `std.vec.new` → `mvp_std::vec::mvp_new`;
                // mangle only the trailing identifier so C++ keywords (new,
                // delete, class, ...) don't collide at the call site.
                let joined = parts.join("::");
                mangle_cpp_kw(&joined)
            }
        }
    }
}

pub(crate) fn cxx_module(name: &str) -> String {
    module_parts(name).join("::")
}

pub(crate) fn module_parts(name: &str) -> Vec<String> {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.first() == Some(&"std") {
        let mut result = vec!["mvp_std".into()];
        result.extend(parts[1..].iter().map(|s| s.to_string()));
        result
    } else if parts.first() == Some(&"main") {
        let mut result = vec!["mvp_main".into()];
        result.extend(parts[1..].iter().map(|s| s.to_string()));
        result
    } else {
        parts.iter().map(|s| s.to_string()).collect()
    }
}

pub(crate) fn cxx_include_path(path: &str) -> String {
    if let Some(c_path) = path.strip_prefix("c:") {
        return format!("#include <{}>\n", c_path);
    }
    let parts: Vec<&str> = path.split('/').collect();
    match crate::config::Config::project_name() {
        Some(proj_name) => {
            if path.starts_with(&proj_name) {
                if parts.len() > 1 {
                    format!("#include <src/{}.miva.h>\n", parts[1..].join("/"))
                } else {
                    String::new()
                }
            } else if let Some(head) = parts.first() {
                if parts.len() > 1 {
                    format!("#include <{}/src/{}.miva.h>\n", head, parts[1..].join("/"))
                } else {
                    format!("#include <{}.miva.h>\n", head)
                }
            } else {
                String::new()
            }
        }
        None => String::new(),
    }
}

pub(crate) fn cxx_include_here(path: &str) -> String {
    let include = cxx_include_path(path);
    if path.starts_with("c:") {
        include
    } else {
        let ns = import_path_to_namespace(path);
        if ns.is_empty() {
            include
        } else {
            format!("{}using namespace {};\n", include, ns)
        }
    }
}

fn import_path_to_namespace(path: &str) -> String {
    if path.starts_with("c:") {
        return String::new();
    }
    if let Some(proj_name) = crate::config::Config::project_name() {
        if let Some(remaining) = path.strip_prefix(&format!("{}/", proj_name)) {
            return cxx_module(&remaining.replace('/', "."));
        }
    }
    cxx_module(&path.replace('/', "."))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== indent_str =====

    #[test]
    fn test_indent_str_zero() {
        assert_eq!(indent_str(0), "");
    }

    #[test]
    fn test_indent_str_one() {
        assert_eq!(indent_str(1), "  ");
    }

    #[test]
    fn test_indent_str_three() {
        assert_eq!(indent_str(3), "      ");
    }

    // ===== cxx_type =====

    #[test]
    fn test_cxx_type_int() {
        assert_eq!(cxx_type(&Typ::TInt), "mvp_builtin_int");
    }

    #[test]
    fn test_cxx_type_bool() {
        assert_eq!(cxx_type(&Typ::TBool), "mvp_builtin_boolean");
    }

    #[test]
    fn test_cxx_type_float64() {
        assert_eq!(cxx_type(&Typ::TFloat64), "mvp_builtin_float");
    }

    #[test]
    fn test_cxx_type_float32() {
        assert_eq!(cxx_type(&Typ::TFloat32), "mvp_builtin_float");
    }

    #[test]
    fn test_cxx_type_char() {
        assert_eq!(cxx_type(&Typ::TChar), "mvp_builtin_byte");
    }

    #[test]
    fn test_cxx_type_string() {
        assert_eq!(cxx_type(&Typ::TString), "mvp_builtin_string");
    }

    #[test]
    fn test_cxx_type_array() {
        let typ = Typ::TArray {
            of: Box::new(Typ::TInt),
        };
        assert_eq!(cxx_type(&typ), "std::vector<mvp_builtin_int>");
    }

    #[test]
    fn test_cxx_type_nested_array() {
        let inner = Typ::TArray {
            of: Box::new(Typ::TInt),
        };
        let outer = Typ::TArray {
            of: Box::new(inner),
        };
        assert_eq!(
            cxx_type(&outer),
            "std::vector<std::vector<mvp_builtin_int>>"
        );
    }

    #[test]
    fn test_cxx_type_struct() {
        let typ = Typ::TStruct {
            name: "Point".into(),
            fields: vec![],
            type_args: vec![],
        };
        assert_eq!(cxx_type(&typ), "Point");
    }

    #[test]
    fn test_cxx_type_ptr() {
        let typ = Typ::TPtr {
            to: Box::new(Typ::TInt),
        };
        assert_eq!(cxx_type(&typ), "mvp_builtin_int*");
    }

    #[test]
    fn test_cxx_type_box() {
        let typ = Typ::TBox {
            of: Box::new(Typ::TInt),
        };
        assert_eq!(cxx_type(&typ), "mvp_builtin_box<mvp_builtin_int>");
    }

    #[test]
    fn test_cxx_type_null() {
        assert_eq!(cxx_type(&Typ::TNull), "void");
    }

    #[test]
    fn test_cxx_type_ptrany() {
        assert_eq!(cxx_type(&Typ::TPtrAny), "mvp_builtin_ptrany");
    }

    #[test]
    fn test_cxx_type_invalid() {
        assert_eq!(cxx_type(&Typ::TInvalid), "invalid");
    }

    // ===== cxx_param =====

    #[test]
    fn test_cxx_param_ref() {
        let p = Param::PRef {
            name: "x".into(),
            typ: Typ::TInt,
        };
        assert_eq!(cxx_param(&p), "mvp_builtin_int const& x");
    }

    #[test]
    fn test_cxx_param_own() {
        let p = Param::POwn {
            name: "flag".into(),
            typ: Typ::TBool,
        };
        assert_eq!(cxx_param(&p), "mvp_builtin_boolean flag");
    }

    #[test]
    fn test_cxx_param_ref_string() {
        let p = Param::PRef {
            name: "s".into(),
            typ: Typ::TString,
        };
        assert_eq!(cxx_param(&p), "mvp_builtin_string const& s");
    }

    // ===== cxx_func_decl =====

    #[test]
    fn test_cxx_func_decl_no_return() {
        let result = cxx_func_decl("foo", &[], &None);
        assert_eq!(result, "mvp_builtin_unit foo();\n");
    }

    #[test]
    fn test_cxx_func_decl_with_return() {
        let result = cxx_func_decl("add", &[], &Some(Typ::TInt));
        assert_eq!(result, "mvp_builtin_int add();\n");
    }

    #[test]
    fn test_cxx_func_decl_with_params() {
        let params = vec![
            Param::POwn {
                name: "a".into(),
                typ: Typ::TInt,
            },
            Param::POwn {
                name: "b".into(),
                typ: Typ::TInt,
            },
        ];
        let result = cxx_func_decl("add", &params, &Some(Typ::TInt));
        assert_eq!(
            result,
            "mvp_builtin_int add(mvp_builtin_int a, mvp_builtin_int b);\n"
        );
    }

    // ===== map_builtin =====

    #[test]
    fn test_map_builtin_print() {
        assert_eq!(map_builtin("print"), "mvp_print");
    }

    #[test]
    fn test_map_builtin_prints() {
        assert_eq!(map_builtin("prints"), "mvp_prints");
    }

    #[test]
    fn test_map_builtin_println() {
        assert_eq!(map_builtin("println"), "mvp_println");
    }

    #[test]
    fn test_map_builtin_printlns() {
        assert_eq!(map_builtin("printlns"), "mvp_printlns");
    }

    #[test]
    fn test_map_builtin_error() {
        assert_eq!(map_builtin("error"), "mvp_error");
    }

    #[test]
    fn test_map_builtin_errors() {
        assert_eq!(map_builtin("errors"), "mvp_errors");
    }

    #[test]
    fn test_map_builtin_errorln() {
        assert_eq!(map_builtin("errorln"), "mvp_errorln");
    }

    #[test]
    fn test_map_builtin_errorlns() {
        assert_eq!(map_builtin("errorlns"), "mvp_errorlns");
    }

    #[test]
    fn test_map_builtin_exit() {
        assert_eq!(map_builtin("exit"), "mvp_exit");
    }

    #[test]
    fn test_map_builtin_abort() {
        assert_eq!(map_builtin("abort"), "mvp_abort");
    }

    #[test]
    fn test_map_builtin_panic() {
        assert_eq!(map_builtin("panic"), "mvp_panic");
    }

    #[test]
    fn test_map_builtin_string_concat() {
        assert_eq!(map_builtin("string_concat"), "mvp_string_concat");
    }

    #[test]
    fn test_map_builtin_string_parse() {
        assert_eq!(map_builtin("string_parse"), "mvp_string_parse");
    }

    #[test]
    fn test_map_builtin_string_length() {
        assert_eq!(map_builtin("string_length"), "mvp_string_length");
    }

    #[test]
    fn test_map_builtin_string_make() {
        assert_eq!(map_builtin("string_make"), "mvp_string_make");
    }

    #[test]
    fn test_map_builtin_string_from() {
        assert_eq!(map_builtin("string_from"), "mvp_to_string");
    }

    #[test]
    fn test_map_builtin_box_new() {
        assert_eq!(map_builtin("box_new"), "mvp_box_new");
    }

    #[test]
    fn test_map_builtin_box_deref() {
        assert_eq!(map_builtin("box_deref"), "mvp_box_deref");
    }

    #[test]
    fn test_map_builtin_range() {
        assert_eq!(map_builtin("range"), "mvp_range");
    }

    #[test]
    fn test_map_builtin_ptr_alloc() {
        assert_eq!(map_builtin("ptr_alloc"), "mvp_alloc");
    }

    #[test]
    fn test_map_builtin_ptr_realloc() {
        assert_eq!(map_builtin("ptr_realloc"), "mvp_realloc");
    }

    #[test]
    fn test_map_builtin_ptr_free() {
        assert_eq!(map_builtin("ptr_free"), "mvp_free");
    }

    #[test]
    fn test_map_builtin_ptr_set() {
        assert_eq!(map_builtin("ptr_set"), "mvp_builtin_ptrset");
    }

    #[test]
    fn test_map_builtin_ptr_offset() {
        assert_eq!(map_builtin("ptr_offset"), "mvp_ptr_offset");
    }

    #[test]
    fn test_map_builtin_ffi() {
        assert_eq!(map_builtin("ffi.some_c_func"), "some_c_func");
    }

    #[test]
    fn test_map_builtin_user_func() {
        assert_eq!(map_builtin("user.func"), "user::func");
    }

    #[test]
    fn test_map_builtin_multi_dot() {
        assert_eq!(map_builtin("a.b.c"), "a::b::c");
    }

    // ===== module_parts =====

    #[test]
    fn test_module_parts_std() {
        assert_eq!(module_parts("std.io"), vec!["mvp_std", "io"]);
    }

    #[test]
    fn test_module_parts_main() {
        assert_eq!(module_parts("main.app"), vec!["mvp_main", "app"]);
    }

    #[test]
    fn test_module_parts_custom() {
        assert_eq!(module_parts("my.module"), vec!["my", "module"]);
    }

    #[test]
    fn test_module_parts_deep() {
        assert_eq!(module_parts("a.b.c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_module_parts_single() {
        assert_eq!(module_parts("foo"), vec!["foo"]);
    }

    // ===== cxx_module =====

    #[test]
    fn test_cxx_module_std() {
        assert_eq!(cxx_module("std.io"), "mvp_std::io");
    }

    #[test]
    fn test_cxx_module_main() {
        assert_eq!(cxx_module("main.app"), "mvp_main::app");
    }

    #[test]
    fn test_cxx_module_custom() {
        assert_eq!(cxx_module("my.module"), "my::module");
    }

    #[test]
    fn test_cxx_module_single() {
        assert_eq!(cxx_module("foo"), "foo");
    }
}
