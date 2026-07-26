# ADR-004: Shape Type Checking and Satisfaction

**Status:** Proposed  
**Date:** 2026-07-26

## Context

Type checking (`miva/src/typecheck.rs`) currently tracks structs via `build_struct_map()` returning `(HashMap<String, Vec<FieldDef>>, HashMap<String, Vec<String>>)`. Shapes need parallel tracking. The type checker must verify shape satisfaction when a `TShape` type is used.

## Decision

### Shape registry

Add `build_shape_map()` parallel to `build_struct_map()`:

```rust
fn build_shape_map(
    defs: &[Def],
) -> (HashMap<String, Vec<FieldDef>>, HashMap<String, Vec<String>>) {
    // Same pattern as build_struct_map but for DShape
}
```

Pass this alongside `structs` and `struct_type_params` to `require_type`/`infer_type`.

### Shape satisfaction check

When the type checker encounters `SLetTyped { typ: TShape{name}, expr }`:

1. Look up the shape's field list from the shape map
2. Infer the expression's type
3. If the expression type is a `TStruct { name: sname, ... }`, look up the struct's fields
4. For each field in the shape, verify it exists in the struct with a compatible type
5. If types involve generic params, resolve them first using the substitution map

The satisfaction function:

```rust
fn satisfies_shape(struct_fields: &HashMap<&str, &Typ>, shape_fields: &[FieldDef], subst: &HashMap<String, Typ>) -> bool {
    for shape_field in shape_fields {
        let resolved_type = if subst.is_empty() {
            &shape_field.typ
        } else {
            &resolve_type(&shape_field.typ, subst)
        };
        match struct_fields.get(shape_field.name.as_str()) {
            Some(&struct_field_type) => {
                if !types_equal(resolved_type, struct_field_type) {
                    return false;
                }
            }
            None => return false, // Missing required field
        }
    }
    true
}
```

### Generic bound checking

When calling a generic function with shape bounds:

```miva
greet[T: nameShape] = (p: T): string => p.name
greet[Person](person)
```

After type inference resolves `T` to `Person` (a `TStruct`), verify that `Person`'s fields satisfy `nameShape`'s fields. If not, emit a type error.

For multiple bounds (`T: nameShape + ageShape`), check all bounds.

### TShape in type normalization

`normalize_typ()` and `resolve_type()` in `typecheck.rs` must handle `TShape`:

- `normalize_typ`: `TShape` passes through unchanged (it has no nested type params to normalize — well, actually it might if shapes have generic params... but `TShape` only stores a name)
- `resolve_type`: `TShape` passes through unchanged
- `types_equal`: Two `TShape` values are equal if their names match

### Handling generic shape parameters in bounds

For `useValue[V: hasValue[int]]`:

1. Parse `hasValue[int]` as the bound — a shape name with type arguments
2. Store in type_bounds as `("V", vec![("hasValue", vec![int])])` or similar structure
3. During resolution, resolve the shape's own type params against the provided args
4. Then check satisfaction using the resolved shape fields

### Cross-module shape satisfaction

When module A defines a shape and module B's struct satisfies it:

1. Shapes are collected into `global_shapes` during Phase 0.5 of build.rs (like `global_enums`)
2. Per-file type checking receives `global_shapes` alongside `global_type_sigs`
3. Shape lookup uses the same qualified name pattern as functions

## Rationale

- Structural satisfaction is checked at the point of use (type annotation or generic instantiation), not at shape definition time
- Using HashMap for field lookup matches the existing struct field access pattern
- Resolution of generic shape params before satisfaction check ensures correct type comparison
