use crate::ast::*;
use std::collections::HashMap;

// ── Generic type helpers ────────────────────────────────────────────────

/// Convert TStruct references to TGenericParam when they match a type parameter name.
pub(crate) fn normalize_typ(typ: &Typ, type_params: &[String]) -> Typ {
    match typ {
        Typ::TStruct { name, .. } if type_params.contains(name) => {
            Typ::TGenericParam { name: name.clone() }
        }
        Typ::TStruct {
            name,
            fields,
            type_args,
        } => Typ::TStruct {
            name: name.clone(),
            fields: fields.clone(),
            type_args: type_args
                .iter()
                .map(|ta| normalize_typ(ta, type_params))
                .collect(),
        },
        Typ::TArray { of } => Typ::TArray {
            of: Box::new(normalize_typ(of, type_params)),
        },
        Typ::TPtr { to } => Typ::TPtr {
            to: Box::new(normalize_typ(to, type_params)),
        },
        Typ::TBox { of } => Typ::TBox {
            of: Box::new(normalize_typ(of, type_params)),
        },
        Typ::TFuture { of } => Typ::TFuture {
            of: Box::new(normalize_typ(of, type_params)),
        },
        Typ::TFunc { params, returns } => Typ::TFunc {
            params: params
                .iter()
                .map(|p| normalize_typ(p, type_params))
                .collect(),
            returns: Box::new(normalize_typ(returns, type_params)),
        },
        _ => typ.clone(),
    }
}

pub(crate) fn normalize_params(params: &[Param], type_params: &[String]) -> Vec<Param> {
    params
        .iter()
        .map(|p| {
            let (name, typ, is_ref) = match p {
                Param::PRef { name, typ } => (name.clone(), typ.clone(), true),
                Param::POwn { name, typ } => (name.clone(), typ.clone(), false),
            };
            let norm_typ = normalize_typ(&typ, type_params);
            if is_ref {
                Param::PRef {
                    name,
                    typ: norm_typ,
                }
            } else {
                Param::POwn {
                    name,
                    typ: norm_typ,
                }
            }
        })
        .collect()
}

/// Resolve type variables to concrete types using a substitution map.
pub(crate) fn resolve_type(typ: &Typ, subst: &HashMap<String, Typ>) -> Typ {
    match typ {
        Typ::TGenericParam { name } => subst.get(name).cloned().unwrap_or(Typ::TInvalid),
        Typ::TStruct {
            name,
            fields,
            type_args,
        } => Typ::TStruct {
            name: name.clone(),
            fields: fields.clone(),
            type_args: type_args.iter().map(|ta| resolve_type(ta, subst)).collect(),
        },
        Typ::TArray { of } => Typ::TArray {
            of: Box::new(resolve_type(of, subst)),
        },
        Typ::TPtr { to } => Typ::TPtr {
            to: Box::new(resolve_type(to, subst)),
        },
        Typ::TBox { of } => Typ::TBox {
            of: Box::new(resolve_type(of, subst)),
        },
        Typ::TFuture { of } => Typ::TFuture {
            of: Box::new(resolve_type(of, subst)),
        },
        Typ::TFunc { params, returns } => Typ::TFunc {
            params: params.iter().map(|p| resolve_type(p, subst)).collect(),
            returns: Box::new(resolve_type(returns, subst)),
        },
        _ => typ.clone(),
    }
}

/// Infer type param bindings from actual argument types.
pub(crate) fn infer_type_from_arg(param_typ: &Typ, arg_typ: &Typ, subst: &mut HashMap<String, Typ>) {
    match (param_typ, arg_typ) {
        (Typ::TGenericParam { name }, _) => {
            if !subst.contains_key(name) {
                subst.insert(name.clone(), arg_typ.clone());
            }
        }
        (Typ::TArray { of: pof }, Typ::TArray { of: aof }) => {
            infer_type_from_arg(pof, aof, subst);
        }
        (Typ::TPtr { to: pto }, Typ::TPtr { to: ato }) => {
            infer_type_from_arg(pto, ato, subst);
        }
        (Typ::TBox { of: pof }, Typ::TBox { of: aof }) => {
            infer_type_from_arg(pof, aof, subst);
        }
        (Typ::TStruct { type_args: pta, .. }, Typ::TStruct { type_args: ata, .. })
            if pta.len() == ata.len() =>
        {
            for (pt, at) in pta.iter().zip(ata.iter()) {
                infer_type_from_arg(pt, at, subst);
            }
        }
        (Typ::TFunc { params: pp, returns: pr }, Typ::TFunc { params: ap, returns: ar })
            if pp.len() == ap.len() =>
        {
            for (pt, at) in pp.iter().zip(ap.iter()) {
                infer_type_from_arg(pt, at, subst);
            }
            infer_type_from_arg(pr, ar, subst);
        }
        _ => {}
    }
}
