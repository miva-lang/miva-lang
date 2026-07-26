# 03 — Shape Symbol Table and Export

**What to build:** Shapes are tracked in the symbol table alongside structs and enums. They can be exported via `export shapeName;` and looked up by name. This enables cross-module shape visibility.

**Blocked by:** None — can start immediately (no runtime dependency on other tickets).

**Status:** ready-for-agent

- [ ] Add `ShapeEntry { name, fields, type_params }` struct in `miva/src/symbol_table.rs`
- [ ] Add `shapes: Vec<ShapeEntry>`, `shape_index: HashMap<String, usize>`, `exported_shapes: Vec<String>` to `SymbolTable`
- [ ] Add `register_shape()` method — mirrors `register_struct()`
- [ ] Add `lookup_shape()` method — mirrors `lookup_struct()`
- [ ] Handle `Def::DShape` in `build_with_errors()` → call `register_shape()`
- [ ] Handle `SExport { symbol }` → check `shape_index` and add to `exported_shapes`
- [ ] Duplicate shape detection (E0004)
- [ ] Run `cargo test` — no regressions
