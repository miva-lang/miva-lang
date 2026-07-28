/// Single source of truth for the MVM builtin name -> id table.
///
/// Shared by the compiler's bytecode emitter (`miva`'s mvm backend), which
/// resolves builtin call names to indices at compile time, and by the VM's
/// runtime name lookup. The ids must match the dispatch order in
/// `Mvm::call_builtin`.
pub const BUILTIN_IDS: &[(&str, u8)] = &[
    ("print", 0), ("prints", 1), ("println", 2), ("printlns", 3),
    ("error", 4), ("errors", 5), ("errorln", 6), ("errorlns", 7),
    ("exit", 8), ("abort", 9), ("panic", 10),
    ("string_concat", 11), ("string_length", 12), ("string_parse", 13),
    ("string_make", 14), ("string_from", 15), ("string_get", 16),
    ("box_new", 17), ("box_deref", 18), ("box_set", 19),
    ("range", 20), ("to_string", 21), ("read_int", 22), ("read_line", 23),
    ("json_parse", 24), ("json_kind", 25), ("json_bool", 26),
    ("json_number", 27), ("json_string", 28), ("json_array_len", 29),
    ("json_array_get", 30), ("json_object_len", 31), ("json_object_key", 32),
    ("json_object_get", 33), ("json_object_find", 34), ("json_free", 35),
    ("json_stringify", 36),
    ("xml_parse", 37), ("xml_kind", 38), ("xml_tag", 39),
    ("xml_attr_count", 40), ("xml_attr_name", 41), ("xml_attr_value", 42),
    ("xml_attr_find", 43), ("xml_child_count", 44), ("xml_child_get", 45),
    ("xml_text", 46), ("xml_comment", 47), ("xml_cdata", 48),
    ("xml_pi_target", 49), ("xml_pi_data", 50), ("xml_stringify", 51),
    ("xml_free", 52),
    ("toml_parse", 53), ("toml_kind", 54), ("toml_bool", 55),
    ("toml_number", 56), ("toml_string", 57), ("toml_array_len", 58),
    ("toml_array_get", 59), ("toml_object_len", 60), ("toml_object_key", 61),
    ("toml_object_get", 62), ("toml_object_find", 63), ("toml_free", 64),
    ("toml_stringify", 65),
    ("yaml_parse", 66), ("yaml_kind", 67), ("yaml_bool", 68),
    ("yaml_number", 69), ("yaml_string", 70), ("yaml_array_len", 71),
    ("yaml_array_get", 72), ("yaml_object_len", 73), ("yaml_object_key", 74),
    ("yaml_object_get", 75), ("yaml_object_find", 76), ("yaml_free", 77),
    ("yaml_stringify", 78),
    ("ptr_alloc", 79), ("ptr_free", 80), ("ptr_realloc", 81),
    ("ptr_offset", 82), ("ptr_set", 83), ("ptr_ref", 84),
    ("mutex_new", 85), ("mutex_lock", 86), ("mutex_unlock", 87),
    ("mutex_free", 88),
];
