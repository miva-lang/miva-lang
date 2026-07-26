# 01 — Shape AST Foundation

**What to build:** Add `DShape` definition and `TShape` type variant to both frontend and compiler AST files, keeping them in sync. This is the foundation that every subsequent ticket depends on.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] Add `DShape { loc, name, fields, type_params }` variant to `Def` enum in `miva-frontend-rs/src/ast.rs`
- [ ] Add `TShape { name }` variant to `Typ` enum in `miva-frontend-rs/src/ast.rs`
- [ ] Mirror both changes in `miva/src/ast.rs` (must stay in sync)
- [ ] Add `type_bounds: Vec<String>` field to `DFunc` in both AST files (for generic bounds syntax)
- [ ] Run `cargo test` in both crates — no regressions
