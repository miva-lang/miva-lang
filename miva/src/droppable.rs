// Shared droppability analysis: which named types need drop glue, and
// whether a given Typ is droppable. Used by semantic (move-only + E0036
// enforcement), typecheck (inferred generic-arg ban), and drop_desugar.

use crate::ast::*;
use std::collections::HashSet;

pub fn is_droppable_typ(droppable: &HashSet<String>, t: &Typ) -> bool {
    match t {
        Typ::TStruct { name, .. } => droppable.contains(name),
        Typ::TArray { of } => is_droppable_typ(droppable, of),
        _ => false,
    }
}

/// Names of all droppable types: those with a registered op_drop, plus —
/// because droppability is infectious — any struct containing a droppable
/// field or enum carrying a droppable payload, at any nesting depth.
pub fn compute_droppable(defs: &[Def]) -> HashSet<String> {
    let mut droppable: HashSet<String> = defs
        .iter()
        .filter_map(|d| match d {
            Def::DImpl {
                struct_name, impls, ..
            } if impls.iter().any(|i| matches!(i.op, ImplOp::ImDrop)) => {
                Some(struct_name.clone())
            }
            _ => None,
        })
        .collect();
    loop {
        let before = droppable.len();
        for def in defs {
            match def {
                Def::DStruct { name, fields, .. } => {
                    if !droppable.contains(name)
                        && fields.iter().any(|f| is_droppable_typ(&droppable, &f.typ))
                    {
                        droppable.insert(name.clone());
                    }
                }
                Def::DEnum { name, variants, .. } => {
                    if !droppable.contains(name)
                        && variants
                            .iter()
                            .any(|v| v.payload.iter().any(|t| is_droppable_typ(&droppable, t)))
                    {
                        droppable.insert(name.clone());
                    }
                }
                _ => {}
            }
        }
        if droppable.len() == before {
            break;
        }
    }
    droppable
}
