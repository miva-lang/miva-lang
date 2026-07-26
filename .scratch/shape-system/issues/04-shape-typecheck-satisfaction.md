# 04 — Shape Type Checking: Satisfaction and First-Class Types

**What to build:** The type checker verifies that a struct satisfies a shape's field requirements. Users can assign structs to shape-typed variables, and the compiler checks structural compatibility.

**Blocked by:** 01 — Shape AST Foundation, 03 — Shape Symbol Table and Export

**Status:** ready-for-agent

- [ ] Add `build_shape_map()` function — mirrors `build_struct_map()`, returns `(shape_name → fields, shape_name → type_params)`
- [ ] Extend `check_program_with()` to accept `shapes` and `shape_type_params` maps
- [ ] Handle `TShape` in `normalize_typ()`, `resolve_type()`, `types_equal()` — pass through or compare by name
- [ ] Implement `satisfies_shape(struct_fields, shape_fields, subst)` — structural comparison
- [ ] Handle `SLetTyped { typ: TShape{name}, expr }`:
  - Infer expression type
  - If `TStruct`, look up struct fields and shape fields
  - Call `satisfies_shape()` with resolved types
  - Emit E0028 on missing field, E0030 on type mismatch
- [ ] Handle non-struct assigned to shape → error
- [ ] Run `cargo test` — verify exact match, superset, missing field, type mismatch cases
