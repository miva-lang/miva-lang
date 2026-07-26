# 06 — Cross-Module Shape Resolution

**What to build:** Shapes exported from one module are visible in other modules. A struct in module B can satisfy a shape defined in module A, enabling modular shape usage.

**Blocked by:** 03 — Shape Symbol Table and Export, 04 — Shape Type Checking: Satisfaction and First-Class Types

**Status:** ready-for-agent

- [ ] In `build.rs` Phase 0.5, collect `global_shapes` and `global_shape_type_params` alongside `global_enums`
- [ ] Qualify shape names using same convention as functions (e.g., `mvp_std.io`, `main.y`)
- [ ] Pass `global_shapes` and `global_shape_type_params` into per-file `check_program_with()`
- [ ] Merge cross-module shapes into per-file check (local takes precedence)
- [ ] Handle semantic analysis for cross-module shape visibility
- [ ] Run `cargo test` — verify cross-module shape satisfaction
