use crate::ast::*;
use crate::codegen::cxx::{cxx_param, cxx_type, cxx_func_decl, mangle_cpp_kw, module_parts, cxx_module, cxx_include_here, cxx_include_path, indent_str, cxx_escape_string, map_builtin, collect_generic_params};
use crate::symbol_table::SymbolTable;
use std::cell::RefCell;
use std::collections::HashMap;

mod emit;
mod lower;
mod optimize;
#[cfg(test)]
mod tests;

pub(crate) use emit::*;
pub(crate) use lower::*;
pub(crate) use optimize::*;

thread_local! {
    static CLOSURE_DEFS: RefCell<HashMap<usize, IrClosureDef>> = RefCell::new(HashMap::new());
    static CLOSURE_ID: RefCell<usize> = RefCell::new(0);
    static ENUM_DEFS: RefCell<HashMap<String, Vec<crate::ast::EnumVariant>>> =
        RefCell::new(HashMap::new());
}

pub(crate) fn next_closure_id() -> usize {
    CLOSURE_ID.with(|c| {
        let mut id = c.borrow_mut();
        let v = *id;
        *id += 1;
        v
    })
}

pub(crate) fn reset_closure_registry() {
    CLOSURE_DEFS.with(|c| c.borrow_mut().clear());
    CLOSURE_ID.with(|c| *c.borrow_mut() = 0);
}

pub(crate) fn emit_closure_def(def: IrClosureDef) {
    CLOSURE_DEFS.with(|c| { c.borrow_mut().insert(def.id, def); });
}

pub(crate) fn take_closure_defs() -> String {
    let closures: Vec<IrClosureDef> = CLOSURE_DEFS.with(|c| {
        let map = c.borrow();
        let mut ids: Vec<_> = map.keys().copied().collect();
        ids.sort();
        ids.into_iter().map(|id| map[&id].clone()).collect()
    });
    let out: String = closures.iter().map(|cl| {
        let env_name = format!("__closure_env_{}", cl.id);
        let env_fields_str: Vec<_> = cl.env_fields.iter().map(|(n, t)| {
            format!("  {} {};", cxx_type(t), n)
        }).collect();
        let env_struct = if cl.env_fields.is_empty() {
            format!("struct {} {{}};", env_name)
        } else {
            format!("struct {} {{\n{}\n}};", env_name, env_fields_str.join("\n"))
        };
        let ret_cxx = cxx_type(&cl.ret_type);
        let body_str = if cl.thunk_body_stmts.is_empty() {
            format!(
                "{{\n  auto& __env = *static_cast<{}*>(__env_ptr);\n  return {};\n}}",
                env_name,
                emit_expr(cl.thunk_body_result.as_ref().unwrap(), 1, Some(&ret_cxx))
            )
        } else {
            let stmt_strs: String = cl.thunk_body_stmts.iter().map(|s| emit_stmt(s, 2)).collect();
            let capture_bindings: Vec<_> = cl.env_fields.iter().map(|(n, _)| format!("  auto& {} = __env.{};\n", n, n)).collect();
            let bind_str: String = capture_bindings.into_iter().collect();
            let env_cast = format!("  auto& __env = *static_cast<{}*>(__env_ptr);\n", env_name);
            let body = format!("{}{}{}", env_cast, bind_str, stmt_strs);
            match cl.thunk_body_result {
                Some(ref e) => {
                    format!("{{\n{}  return {};\n}}", body, emit_expr(e, 2, Some(&ret_cxx)))
                }
                None => format!("{{\n{}}}", body),
            }
        };
        let thunk = format!(
            "static {} __closure_thunk_{}(void* __env_ptr{}) {}\n",
            ret_cxx, cl.id, if cl.param_list.is_empty() { String::new() } else { format!(", {}", cl.param_list) }, body_str
        );
        format!("{}\n{}", env_struct, thunk)
    }).collect::<Vec<_>>().join("\n\n");
    CLOSURE_DEFS.with(|c| c.borrow_mut().clear());
    out
}

pub(crate) fn record_enum_defs(defs: &[Def]) {
    ENUM_DEFS.with(|m| {
        let mut map = m.borrow_mut();
        map.clear();
        for d in defs {
            if let Def::DEnum { name, variants, .. } = d {
                map.insert(name.clone(), variants.clone());
            }
        }
    });
}

pub(crate) fn first_payload_variant(enum_name: &str) -> Option<String> {
    ENUM_DEFS.with(|m| {
        m.borrow().get(enum_name).and_then(|variants| {
            variants
                .iter()
                .find(|v| !v.payload.is_empty())
                .map(|v| v.name.clone())
        })
    })
}

pub(crate) fn enum_payload_field_ref(var_str: &str, enum_name: &str, variant: &str, field: usize) -> String {
    if first_payload_variant(enum_name).as_deref() == Some(variant) {
        format!("{}.__payload.field{}", var_str, field)
    } else {
        format!("{}.__payload.{}.field{}", var_str, variant, field)
    }
}

pub(crate) fn is_panic(e: &Expr) -> bool {
    match e {
        Expr::ECall { name, .. } if name == "panic" => true,
        Expr::EBlock { stmts, result, .. } => {
            (result.is_none()
                && stmts.iter().any(|s| match s {
                    Stmt::SExpr { expr, .. } => is_panic(expr),
                    Stmt::SReturn { expr, .. } => is_panic(expr),
                    _ => false,
                }))
            || result.as_ref().map_or(false, |r| is_panic(r))
        }
        _ => false,
    }
}

pub(crate) fn is_panic_expr(e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { name, .. } if name == "panic" => true,
        IrExpr::Block { stmts, result, .. } => {
            (result.is_none()
                && stmts.iter().any(|s| match s {
                    IrStmt::Expr(e) => is_panic_expr(e),
                    IrStmt::Return(e) => is_panic_expr(e),
                    _ => false,
                }))
            || result.as_ref().map_or(false, |r| is_panic_expr(r))
        }
        _ => false,
    }
}

#[derive(Debug, Clone)]
pub enum IrExpr {
    Int(i64),
    Bool(bool),
    Float(f64),
    Char(String),
    String(String),
    Void,
    Var(String),
    Move(String),
    Clone(String),
    Call { name: String, type_args: Vec<Typ>, args: Vec<IrExpr> },
    MethodCall { target: Box<IrExpr>, method: String, type_args: Vec<Typ>, args: Vec<IrExpr> },
    BinOp { op: BinOp, left: Box<IrExpr>, right: Box<IrExpr> },
    FieldAccess { expr: Box<IrExpr>, field: String },
    StructInit { name: String, type_args: Vec<Typ>, fields: Vec<(String, IrExpr)> },
    ArrayInit(Vec<IrExpr>),
    TupleInit(Vec<IrExpr>),
    Cast { expr: Box<IrExpr>, to: Typ },
    Addr(Box<IrExpr>),
    Deref(Box<IrExpr>),
    IfValue { cond: Box<IrExpr>, then: Box<IrExpr>, else_: Option<Box<IrExpr>>, has_panic: bool },
    Block { stmts: Vec<IrStmt>, result: Option<Box<IrExpr>> },
    While { cond: Box<IrExpr>, body: Vec<IrStmt>, result: Option<Box<IrExpr>> },
    Loop { body: Vec<IrStmt>, result: Option<Box<IrExpr>> },
    For { var: String, range: Box<IrExpr>, body: Vec<IrStmt>, result: Option<Box<IrExpr>> },
    ClosureRef { id: usize },
    Choose { var: Box<IrExpr>, cases: Vec<IrCase>, otherwise: Option<Box<IrExpr>>, has_panic: bool },
    Macro { name: String, args: Vec<IrExpr> },
}

#[derive(Debug, Clone)]
pub enum IrStmt {
    Let { mutable: bool, name: String, expr: IrExpr },
    LetTyped { name: String, typ: Typ, expr: IrExpr },
    Return(IrExpr),
    Expr(IrExpr),
    Assign { name: String, expr: IrExpr },
    FieldAssign { target: IrExpr, field: String, expr: IrExpr },
    Empty,
    If { cond: IrExpr, then: Vec<IrStmt>, else_: Vec<IrStmt> },
    While { cond: IrExpr, body: Vec<IrStmt> },
    Loop { body: Vec<IrStmt> },
    For { var: String, range: IrExpr, body: Vec<IrStmt> },
}

#[derive(Debug, Clone)]
pub struct IrCase {
    pub pattern: IrPattern,
    pub guard: Option<IrExpr>,
    pub then: IrExpr,
}

#[derive(Debug, Clone)]
pub enum IrPattern {
    EnumTag { enum_name: String, variant: String, bindings: Vec<String> },
    Value(IrExpr),
}

#[derive(Debug, Clone)]
pub enum IrDef {
    Struct { name: String, type_params: Vec<String>, fields: Vec<FieldDef> },
    Enum { name: String, type_params: Vec<String>, variants: Vec<EnumVariant> },
    Func { name: String, type_params: Vec<String>, params: Vec<Param>, returns: Option<Typ>, body_stmts: Vec<IrStmt>, body_result: Option<IrExpr>, is_async: bool },
    AsyncFunc { name: String, type_params: Vec<String>, params: Vec<Param>, returns: Option<Typ>, body_stmts: Vec<IrStmt>, body_result: Option<IrExpr> },
    CFunc { name: String, params: Vec<Param>, returns: Option<Typ>, code: String },
    Test { name: String, body_stmts: Vec<IrStmt>, body_result: Option<IrExpr> },
    Impl { struct_name: String, impls: Vec<ImplExpr> },
    Module { name: String, defs: Vec<IrDef> },
    Export(String),
    Import { path: String },
    ImportAs { path: String, alias: String },
    ImportHere { path: String },
    CMagical { content: String },
    CIntro { content: String },
}

#[derive(Debug, Clone)]
pub struct IrClosureDef {
    pub id: usize,
    pub env_fields: Vec<(String, Typ)>,
    pub thunk_sig: String,
    pub param_list: String,
    pub param_types: Vec<String>,
    pub thunk_body_stmts: Vec<IrStmt>,
    pub thunk_body_result: Option<IrExpr>,
    pub ret_type: Typ,
}

pub struct IrContext {
    pub closures: Vec<IrClosureDef>,
    next_id: usize,
}

impl IrContext {
    pub fn new() -> Self {
        Self {
            closures: Vec::new(),
            next_id: 0,
        }
    }

    fn next_id(&mut self) -> usize {
        let v = self.next_id;
        self.next_id += 1;
        v
    }
}

// ===== LOWERING: AST → IR =====

pub fn build_ir(defs: &[Def]) -> [String; 3] {
    record_enum_defs(defs);
    reset_closure_registry();

    let mut ctx = IrContext::new();
    let ir_defs: Vec<IrDef> = lower_defs(&mut ctx, defs);
    let ir_defs = optimize_defs(ir_defs);

    let header_content = generate_header(&ir_defs);
    let scope_parts = generate_with_scope(&ir_defs, None);
    let closure_defs = take_closure_defs();
    let preamble = "\
#include <iostream>
#include <string>
#include <vector>
#include <cstdint>
#include <type_traits>
#include <mvp_builtin.h>

template<class R, class... A>
struct mvp_closure {
    void* env;
    R (*fn)(void*, A...);
    void (*dtor)(void*);

    ~mvp_closure() { if (env && dtor) dtor(env); }
    mvp_closure() : env(nullptr), fn(nullptr), dtor(nullptr) {}
    mvp_closure(void* e, R(*f)(void*, A...), void(*d)(void*)) : env(e), fn(f), dtor(d) {}
    mvp_closure(mvp_closure&& o) : env(o.env), fn(o.fn), dtor(o.dtor) { o.env = nullptr; }
    mvp_closure(const mvp_closure&) = delete;
    mvp_closure& operator=(mvp_closure&& o) { if (this != &o) { this->~mvp_closure(); env = o.env; fn = o.fn; dtor = o.dtor; o.env = nullptr; } return *this; }
    mvp_closure& operator=(const mvp_closure&) = delete;
    R operator()(A... a) const { return fn(env, a...); }
};


using namespace std;

";
    let program = format!(
        "{}{}{}\n{}\n{}\n",
        preamble, scope_parts.includes, closure_defs, scope_parts.defs_str, scope_parts.main_functions
    );
    let test = generate_test(&ir_defs);
    [program, header_content, test]
}
