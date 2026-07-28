use super::*;

fn loc() -> Loc {
    Loc { line: 1, col: 1 }
}

fn make_module(name: &str) -> Def {
    Def::DModule {
        loc: loc(),
        name: name.to_string(),
    }
}

fn make_func(
    name: &str,
    params: Vec<Param>,
    returns: Option<Typ>,
    body: Expr,
    safety: Safety,
) -> Def {
    Def::DFunc {
        loc: loc(),
        name: name.to_string(),
        type_params: vec![],
        params,
        returns,
        body: Box::new(body),
        safety,
        is_async: false,
        type_bounds: vec![],
    }
}

fn make_struct(name: &str, fields: Vec<FieldDef>) -> Def {
    Def::DStruct {
        loc: loc(),
        name: name.to_string(),
        fields,
        type_params: vec![],
    }
}

mod part1;
mod part2;
