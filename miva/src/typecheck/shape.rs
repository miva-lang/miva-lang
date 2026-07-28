use crate::ast::*;
use crate::error::Error;
use std::collections::HashMap;
use super::*;

/// Check if a struct's fields satisfy a shape's field requirements.
/// Returns (satisfied: bool, missing_field: Option<String>, type_mismatch: Option<(String, String, String)>).
pub(crate) fn satisfies_shape(
    struct_fields: &HashMap<&str, &Typ>,
    shape_fields: &[FieldDef],
    subst: &HashMap<String, Typ>,
) -> (bool, Option<String>, Option<(String, String, String)>) {
    for sf in shape_fields {
        let resolved_type = if subst.is_empty() {
            &sf.typ
        } else {
            &resolve_type(&sf.typ, subst)
        };
        match struct_fields.get(sf.name.as_str()) {
            Some(struct_field_type) => {
                if !types_equal(resolved_type, struct_field_type) {
                    return (false, None, Some((
                        sf.name.clone(),
                        format!("{:?}", resolved_type),
                        format!("{:?}", struct_field_type),
                    )));
                }
            }
            None => return (false, Some(sf.name.clone()), None),
        }
    }
    (true, None, None)
}

/// Walk an expression tree and check SLetTyped statements with TShape types.
pub(crate) fn check_shape_satisfaction(expr: &Expr, shapes: &HashMap<String, Vec<FieldDef>>, structs: &HashMap<String, Vec<FieldDef>>) -> Vec<Error> {
    let mut errs = Vec::new();
    match expr {
        Expr::EBlock { stmts, result, .. } => {
            for stmt in stmts {
                if let Stmt::SLetTyped { name: _, typ, expr: inner_expr, loc } = stmt {
                    if let Typ::TShape { name: shape_name } = typ {
                        if let Some(shape_fields) = shapes.get(shape_name) {
                            let inner_typ = infer_simple_type(inner_expr);
                            if let Some(struct_name) = inner_typ {
                                if let Some(struct_fields) = structs.get(&struct_name) {
                                    let sfm: HashMap<&str, &Typ> = struct_fields.iter()
                                        .map(|f| (f.name.as_str(), &f.typ))
                                        .collect();
                                    let (ok, missing, mismatch) = satisfies_shape(&sfm, shape_fields, &HashMap::new());
                                    if !ok {
                                        if let Some(field) = missing {
                                            errs.push(Error::new("E0028", loc, &format!("type '{}' does not satisfy shape '{}': missing field '{}'", struct_name, shape_name, field)));
                                        } else if let Some((field, expected, actual)) = mismatch {
                                            errs.push(Error::new("E0030", loc, &format!("type '{}' does not satisfy bound '{}': field '{}' has type {} but expected {}", struct_name, shape_name, field, actual, expected)));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let mut sub_errs = check_stmt_satisfaction(stmt, shapes, structs);
                errs.append(&mut sub_errs);
            }
            if let Some(r) = result {
                let mut sub_errs = check_shape_satisfaction(r, shapes, structs);
                errs.append(&mut sub_errs);
            }
        }
        _ => {}
    }
    errs
}

pub(crate) fn check_stmt_satisfaction(stmt: &Stmt, shapes: &HashMap<String, Vec<FieldDef>>, structs: &HashMap<String, Vec<FieldDef>>) -> Vec<Error> {
    let mut errs = Vec::new();
    match stmt {
        Stmt::SLetTyped { typ, expr, loc, .. } => {
            if let Typ::TShape { name: shape_name } = typ {
                if let Some(shape_fields) = shapes.get(shape_name) {
                    let inner_typ = infer_simple_type(expr);
                    if let Some(struct_name) = inner_typ {
                        if let Some(struct_fields) = structs.get(&struct_name) {
                            let sfm: HashMap<&str, &Typ> = struct_fields.iter()
                                .map(|f| (f.name.as_str(), &f.typ))
                                .collect();
                            let (ok, missing, mismatch) = satisfies_shape(&sfm, shape_fields, &HashMap::new());
                            if !ok {
                                if let Some(field) = missing {
                                    errs.push(Error::new("E0028", loc, &format!("type '{}' does not satisfy shape '{}': missing field '{}'", struct_name, shape_name, field)));
                                } else if let Some((field, expected, actual)) = mismatch {
                                    errs.push(Error::new("E0030", loc, &format!("type '{}' does not satisfy bound '{}': field '{}' has type {} but expected {}", struct_name, shape_name, field, actual, expected)));
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    errs
}

/// Simple type inference for expressions (not full type checking).
pub(crate) fn infer_simple_type(expr: &Expr) -> Option<String> {
    match expr {
        Expr::EStructLit { name, .. } => Some(name.clone()),
        Expr::EVar { name, .. } => Some(name.clone()),
        _ => None,
    }
}

