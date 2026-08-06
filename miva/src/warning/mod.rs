#![allow(dead_code)]

use crate::ast::*;
use crate::symbol_table::SymbolTable;

/// A compiler warning with code and human-readable message.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub code: String,
    pub message: String,
    pub loc: Loc,
}

impl Warning {
    fn new(code: &str, loc: &Loc, msg: &str) -> Self {
        Warning {
            code: code.to_string(),
            message: msg.to_string(),
            loc: loc.clone(),
        }
    }
}

/// Format a compiler warning in Rust-style with source code context.
pub fn format_warning_with_source(warn: &Warning, file_path: &str, source: &str) -> String {
    crate::error::format_diagnostic_with_source(
        "warning",
        &warn.code,
        &warn.message,
        &warn.loc,
        file_path,
        source,
    )
}

// ---------------------------------------------------------------------------
// Naming convention helpers (ported from miva-raw/lib/util.ml)
// ---------------------------------------------------------------------------

fn is_uppercase(c: char) -> bool {
    c.is_ascii_uppercase()
}

fn is_lowercase(c: char) -> bool {
    c.is_ascii_lowercase()
}

fn is_lowercase_or_dot(c: char) -> bool {
    is_lowercase(c) || c == '.'
}

fn check_snake(loc: &Loc, name: &str, typ: &str) -> Option<Warning> {
    if name.chars().any(is_uppercase) {
        Some(Warning::new(
            "W0001",
            loc,
            &format!("The {} name '{}' isn't a snake_case name.", typ, name),
        ))
    } else {
        None
    }
}

fn check_all_lower(loc: &Loc, name: &str, typ: &str) -> Option<Warning> {
    if name.chars().any(|c| !is_lowercase_or_dot(c)) {
        Some(Warning::new(
            "W0001",
            loc,
            &format!("The {} name '{}' isn't a lowercase name.", typ, name),
        ))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Deprecation checks (ported from miva-raw/lib/global.ml)
// ---------------------------------------------------------------------------

fn deprecated_func(name: &str, modname: &str) -> Option<String> {
    let msg = |dep: &str, replacement: &str, is_macro: bool| {
        let kind = if is_macro { "macro" } else { "function" };
        format!(
            "\"{}\" is deprecated, use {} \"{}\" instead",
            dep, kind, replacement
        )
    };
    let notrec = |dep: &str, replacement: &str, is_macro: bool| {
        let kind = if is_macro { "macro" } else { "function" };
        format!(
            "\"{}\" is not recommended, use {} \"{}\" instead",
            dep, kind, replacement
        )
    };
    let msg_if = |dep: &str, replacement: &str, is_macro: bool, exclude: &str| {
        if modname == exclude {
            None
        } else {
            Some(msg(dep, replacement, is_macro))
        }
    };
    let notrec_if = |dep: &str, replacement: &str, is_macro: bool, exclude: &str| {
        if modname == exclude {
            None
        } else {
            Some(notrec(dep, replacement, is_macro))
        }
    };

    match name {
        "prints" => Some(msg("prints", "prints", true)),
        "printlns" => Some(msg("printlns", "printlns", true)),
        "string_concat" => msg_if("string_concat", "std.str.concat", false, "std.str"),
        "string_parse" => msg_if("string_parse", "std.str.parse_int", false, "std.str"),
        "string_length" => msg_if("string_length", "std.str.len", false, "std.str"),
        "string_make" => msg_if("string_make", "std.str.make", false, "std.str"),
        "ptr_alloc" => notrec_if("alloc", "std.mem.alloc", false, "std.mem"),
        "ptr_realloc" => notrec_if("realloc", "std.mem.realloc", false, "std.mem"),
        "ptr_free" => notrec_if("free", "std.mem.free", false, "std.mem"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Deprecated keyword checks
// ---------------------------------------------------------------------------

fn deprecated_keyword() -> &'static str {
    "\"c\" is a deprecated keyword, use \"inline\" instead"
}

// ---------------------------------------------------------------------------
// Core warning generation (ported from miva-raw/lib/warnings.ml)
// ---------------------------------------------------------------------------

fn check_expr(expr: &Expr, modname: &str, warnings: &mut Vec<Warning>) {
    match expr {
        Expr::ECall {
            loc,
            name,
            type_args: _,
            args,
        } => {
            if let Some(msg) = deprecated_func(name, modname) {
                warnings.push(Warning::new("W0002", loc, &msg));
            }
            for arg in args {
                check_expr(arg, modname, warnings);
            }
        }
        Expr::EMacro { .. } | Expr::EMacroVar { .. } => {
            // No-op: macros/macro-vars are skipped
        }
        Expr::EBinOp { left, right, .. } => {
            check_expr(left, modname, warnings);
            check_expr(right, modname, warnings);
        }
        Expr::EIf {
            cond, then, else_, ..
        } => {
            check_expr(cond, modname, warnings);
            check_expr(then, modname, warnings);
            if let Some(e) = else_ {
                check_expr(e, modname, warnings);
            }
        }
        Expr::EChoose {
            var,
            cases,
            otherwise,
            ..
        } => {
            check_expr(var, modname, warnings);
            for case in cases {
                check_expr(&case.when, modname, warnings);
                if let Some(g) = &case.guard {
                    check_expr(g, modname, warnings);
                }
                check_expr(&case.then, modname, warnings);
            }
            if let Some(e) = otherwise {
                check_expr(e, modname, warnings);
            }
        }
        Expr::EFieldAccess { expr, .. } => {
            check_expr(expr, modname, warnings);
        }
        Expr::EStructLit { fields, .. } => {
            for vf in fields {
                check_expr(&vf.value, modname, warnings);
            }
        }
        Expr::EBlock { stmts, result, .. } => {
            for stmt in stmts {
                check_stmt(stmt, modname, warnings);
            }
            if let Some(e) = result {
                check_expr(e, modname, warnings);
            }
        }
        Expr::EArrayLit { values, .. } => {
            for elem in values {
                check_expr(elem, modname, warnings);
            }
        }
        Expr::ETupleLit { values, .. } => {
            for elem in values {
                check_expr(elem, modname, warnings);
            }
        }
        Expr::ECast { expr, .. } => {
            check_expr(expr, modname, warnings);
        }
        Expr::EWhile { cond, body, .. } => {
            check_expr(cond, modname, warnings);
            check_expr(body, modname, warnings);
        }
        Expr::ELoop { body, .. } => {
            check_expr(body, modname, warnings);
        }
        Expr::EFor { range, body, .. } => {
            check_expr(range, modname, warnings);
            check_expr(body, modname, warnings);
        }
        Expr::EAddr { expr, .. } => {
            check_expr(expr, modname, warnings);
        }
        Expr::EDeref { expr, .. } => {
            check_expr(expr, modname, warnings);
        }
        Expr::EInt { .. }
        | Expr::EFloat { .. }
        | Expr::EString { .. }
        | Expr::EBool { .. }
        | Expr::EChar { .. }
        | Expr::EVoid { .. }
        | Expr::EClone { .. }
        | Expr::EMove { .. }
        | Expr::EVar { .. }
        | Expr::EEnumPattern { .. }
        | Expr::ELambda { .. }
        | Expr::ETupleLit { .. } => {}
        Expr::EMethodCall { .. } => unreachable!(),
    }
}

fn check_stmt(stmt: &Stmt, modname: &str, warnings: &mut Vec<Warning>) {
    match stmt {
        Stmt::SLetTuple {
            loc,
            patterns,
            expr,
            ..
        } => {
            for name in patterns {
                if let Some(w) = check_snake(loc, name, "var") {
                    warnings.push(w);
                }
            }
            check_expr(expr, modname, warnings);
        }
        Stmt::SLet {
            loc, name, expr, ..
        } => {
            if let Some(w) = check_snake(loc, name, "var") {
                warnings.push(w);
            }
            check_expr(expr, modname, warnings);
        }
        Stmt::SLetTyped {
            loc, name, expr, ..
        } => {
            if let Some(w) = check_snake(loc, name, "var") {
                warnings.push(w);
            }
            check_expr(expr, modname, warnings);
        }
        Stmt::SAssign { expr, .. } => {
            check_expr(expr, modname, warnings);
        }
        Stmt::SFieldAssign { target, expr, .. } => {
            check_expr(target, modname, warnings);
            check_expr(expr, modname, warnings);
        }
        Stmt::SReturn { expr, .. } => {
            check_expr(expr, modname, warnings);
        }
        Stmt::SExpr { expr, .. } => {
            check_expr(expr, modname, warnings);
        }
        Stmt::SCIntro { loc, content } => {
            // Bug-for-bug compat: the OCaml original always warns for SCIntro
            let s = content.trim();
            let sd: Vec<&str> = s.split(':').collect();
            if sd.len() < 2 {
                warnings.push(Warning::new("W0003", loc, "intro comments isn't valid"));
            } else {
                warnings.push(Warning::new(
                    "W0003",
                    loc,
                    &format!("invalid intro comment {}", sd[0]),
                ));
            }
        }
        Stmt::SEmpty { .. } => {}
    }
}

// ---------------------------------------------------------------------------
// Annotation-type checking (ported from miva-raw/lib/anoncheck.ml)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum AnonTyp {
    Impl,
    Unsafe,
    Trusted,
    Usage,
    Param,
    Invalid,
}

/// Parse the annotation type from the content of a DCIntro.
///
/// "unsafe: raw memory op" → AnonTyp::Unsafe
/// "  usage  : used in main" → AnonTyp::Usage (whitespace trimmed)
/// "just_text" → AnonTyp::Invalid (no colon, whole string treated as type)
fn typ_of_anon(s: &str) -> AnonTyp {
    let s = s.trim();
    let typ = s.split(':').next().unwrap_or("").trim();
    match typ {
        "impl" => AnonTyp::Impl,
        "unsafe" => AnonTyp::Unsafe,
        "trusted" => AnonTyp::Trusted,
        "usage" => AnonTyp::Usage,
        "param" => AnonTyp::Param,
        _ => AnonTyp::Invalid,
    }
}

/// Check that DCIntro annotations have valid types for the following definition.
///
/// Ported from `Anoncheck.check_anon` in `miva-raw/lib/anoncheck.ml`.
fn check_annotations(defs: &[Def]) -> Vec<Warning> {
    use AnonTyp::*;
    let mut warnings = Vec::new();
    let mut prev: Option<&Def> = None;

    for cur in defs {
        let prev_dcintro = match prev {
            Some(Def::DCIntro { content, .. }) => Some(content.as_str()),
            _ => None,
        };

        if let Some(anno_str) = prev_dcintro {
            let anno = typ_of_anon(anno_str);
            let loc = match cur {
                Def::DFunc { loc, .. }
                | Def::DCFuncUnsafe { loc, .. }
                | Def::DTest { loc, .. }
                | Def::DStruct { loc, .. }
                | Def::DModule { loc, .. }
                | Def::SExport { loc, .. }
                | Def::SImport { loc, .. }
                | Def::SImportAs { loc, .. }
                | Def::SImportHere { loc, .. }
                | Def::DImpl { loc, .. }
                | Def::DMacro { loc, .. }
                | Def::DEnum { loc, .. }
                | Def::DShape { loc, .. } => loc,
                // DCMagical and DCIntro don't need annotations
                Def::DCMagical { .. } | Def::DCIntro { .. } => {
                    prev = Some(cur);
                    continue;
                }
            };

            let valid = match cur {
                Def::DFunc { safety, .. } => match safety {
                    Safety::Safe => matches!(anno, Usage | Param),
                    Safety::Unsafe => matches!(anno, Unsafe | Usage | Param),
                    Safety::Trusted => matches!(anno, Trusted | Usage | Param),
                },
                Def::DCFuncUnsafe { .. } => matches!(anno, Unsafe | Usage | Param),
                Def::DTest { .. } => matches!(anno, Usage),
                Def::DStruct { .. } => matches!(anno, Usage | Impl),
                // Module/import/export/impl always warn
                Def::DModule { .. }
                | Def::SExport { .. }
                | Def::SImport { .. }
                | Def::SImportAs { .. }
                | Def::SImportHere { .. }
                | Def::DImpl { .. }
                | Def::DMacro { .. }
                | Def::DEnum { .. }
                | Def::DShape { .. } => false,
                // DCMagical/DCIntro already handled above (skipped)
                Def::DCMagical { .. } | Def::DCIntro { .. } => unreachable!(),
            };

            if !valid {
                warnings.push(Warning::new("W0003", loc, "invalid intro comment type"));
            }
        }

        prev = Some(cur);
    }

    warnings
}

/// Collect all warnings from a list of top-level definitions.
///
/// This is the main entry point, mirroring `Warnings.get_warnings` from the
/// OCaml `miva-raw/lib/warnings.ml`.
pub fn get_warnings(defs: &[Def]) -> Vec<Warning> {
    let symt = SymbolTable::build(defs);
    let modname = &symt.module_name;
    let mut warnings = Vec::new();

    for def in defs {
        match def {
            Def::DFunc {
                loc, name, body, ..
            } => {
                if let Some(w) = check_snake(loc, name, "function") {
                    warnings.push(w);
                }
                check_expr(body, modname, &mut warnings);
            }
            Def::DModule { loc, name } => {
                if let Some(w) = check_all_lower(loc, name, "module") {
                    warnings.push(w);
                }
            }
            Def::DCFuncUnsafe {
                loc,
                used_c_keyword: true,
                ..
            } => {
                warnings.push(Warning::new("W0004", loc, deprecated_keyword()));
            }
            Def::DStruct { .. }
            | Def::DTest { .. }
            | Def::DCFuncUnsafe { .. }
            | Def::SExport { .. }
            | Def::SImport { .. }
            | Def::SImportAs { .. }
            | Def::SImportHere { .. }
            | Def::DCMagical { .. }
            | Def::DCIntro { .. }
            | Def::DImpl { .. }
            | Def::DMacro { .. }
            | Def::DEnum { .. }
            | Def::DShape { .. } => {}
        }
    }

    let mut anno_warnings = check_annotations(defs);
    warnings.append(&mut anno_warnings);

    warnings
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
