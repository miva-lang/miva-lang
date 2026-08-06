use super::*;
use crate::ast::*;
use std::collections::HashMap;

pub(crate) fn builtin_return_typ(name: &str) -> Option<Typ> {
    match name {
        "print" | "prints" | "println" | "printlns" | "error" | "errors" | "errorln"
        | "errorlns" | "exit" | "abort" | "panic" | "ptr_free" | "ptr_set" => Some(Typ::TNull),
        "string_concat" | "string_make" | "string_from" => Some(Typ::TString),
        "string_parse" | "string_length" => Some(Typ::TInt),
        "range" => Some(Typ::TArray {
            of: Box::new(Typ::TInt),
        }),
        "ptr_alloc" | "ptr_realloc" | "ptr_offset" => Some(Typ::TPtrAny),
        "box_new" | "box_deref" => None,
        "json_parse" | "json_array_get" | "json_object_get" | "json_object_find" => {
            Some(Typ::TPtrAny)
        }
        "json_kind" | "json_array_len" | "json_object_len" => Some(Typ::TInt),
        "json_bool" => Some(Typ::TBool),
        "json_number" => Some(Typ::TFloat64),
        "json_string" | "json_stringify" => Some(Typ::TString),
        "xml_parse" | "xml_child_get" => Some(Typ::TPtrAny),
        "xml_kind" | "xml_attr_count" | "xml_child_count" => Some(Typ::TInt),
        "xml_tag" | "xml_attr_name" | "xml_attr_value" | "xml_attr_find" | "xml_text"
        | "xml_comment" | "xml_cdata" | "xml_pi_target" | "xml_pi_data" | "xml_stringify" => {
            Some(Typ::TString)
        }
        "xml_free" => Some(Typ::TNull),
        "toml_parse" | "toml_array_get" | "toml_object_get" | "toml_object_find" => {
            Some(Typ::TPtrAny)
        }
        "toml_kind" | "toml_array_len" | "toml_object_len" => Some(Typ::TInt),
        "toml_bool" => Some(Typ::TBool),
        "toml_number" => Some(Typ::TFloat64),
        "toml_string" | "toml_stringify" => Some(Typ::TString),
        "toml_free" => Some(Typ::TNull),
        "yaml_parse" | "yaml_array_get" | "yaml_object_get" | "yaml_object_find" => {
            Some(Typ::TPtrAny)
        }
        "yaml_kind" | "yaml_array_len" | "yaml_object_len" => Some(Typ::TInt),
        "yaml_bool" => Some(Typ::TBool),
        "yaml_number" => Some(Typ::TFloat64),
        "yaml_string" | "yaml_stringify" => Some(Typ::TString),
        "yaml_free" => Some(Typ::TNull),
        "mutex_new" => Some(Typ::TPtrAny),
        "mutex_lock" | "mutex_unlock" | "mutex_free" => Some(Typ::TNull),
        _ => None,
    }
}

pub(crate) fn build_func_sigs(
    defs: &[Def],
) -> (
    HashMap<String, (Vec<String>, Vec<Param>, Option<Typ>)>,
    HashMap<String, Vec<String>>,
) {
    let mut sigs = HashMap::new();
    let mut type_bounds_map = HashMap::new();
    for def in defs {
        match def {
            Def::DFunc {
                name,
                type_params,
                params,
                returns,
                type_bounds,
                ..
            } => {
                // Normalize generic types so func_sigs contains TGenericParam
                // instead of TStruct for generic type parameter names
                let normalized_type_params = type_params.clone();
                let normalized_params = normalize_params(params, type_params);
                let normalized_returns = returns.as_ref().map(|r| normalize_typ(r, type_params));
                sigs.insert(
                    name.clone(),
                    (
                        normalized_type_params,
                        normalized_params,
                        normalized_returns,
                    ),
                );
                // Also store type bounds for shape checking
                if !type_bounds.is_empty() {
                    type_bounds_map.insert(name.clone(), type_bounds.clone());
                }
            }
            Def::DCFuncUnsafe {
                name,
                params,
                returns,
                ..
            } => {
                sigs.insert(name.clone(), (vec![], params.clone(), returns.clone()));
            }
            _ => {}
        }
    }
    (sigs, type_bounds_map)
}
