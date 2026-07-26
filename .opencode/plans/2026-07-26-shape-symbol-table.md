# ADR-005: Shape Symbol Table and Cross-Module Resolution

**Status:** Proposed  
**Date:** 2026-07-26

## Context

The symbol table (`miva/src/symbol_table.rs`) tracks functions, structs, and enums. Shapes need parallel tracking for cross-module resolution and semantic analysis.

## Decision

### New `ShapeEntry` struct

```rust
#[derive(Debug, Clone)]
pub struct ShapeEntry {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub type_params: Vec<String>,
}
```

Identical structure to `StructEntry` but semantically distinct.

### SymbolTable extensions

Add to `SymbolTable`:

```rust
pub shapes: Vec<ShapeEntry>,
pub shape_index: HashMap<String, usize>,
pub exported_shapes: Vec<String>,
```

Add methods:
- `register_shape(name, type_params, fields, loc, errors)` — mirrors `register_struct`
- `lookup_shape(name)` — mirrors `lookup_struct`
- Handle `Def::DShape` in `build_with_errors()` — register the shape
- Handle exported shapes in `SExport` — add to `exported_shapes` (currently only functions are tracked)

### Cross-module collection in build.rs

In Phase 0.5 (line ~676), alongside `global_enums`, collect `global_shapes`:

```rust
let mut global_shapes: HashMap<String, Vec<FieldDef>> = HashMap::new();
let mut global_shape_type_params: HashMap<String, Vec<String>> = HashMap::new();
```

For each `DShape` definition, qualify its name using the same module naming convention as functions:

```rust
// Same qualification logic as global_type_sigs
let qual = format!("{}.{}", qual_prefix, shape_name);
global_shapes.insert(qual.clone(), fields.clone());
global_shape_type_params.insert(qual, type_params.clone());
```

Pass these into per-file type checking via `check_program_with()`.

### Semantic analysis integration

In `semantic.rs`, when a variable is declared with a shape type:

1. Look up the shape in the symbol table (local + imported)
2. If no local match, check `global_shapes` for cross-module shapes
3. The actual satisfaction check happens in type checking, not semantic analysis
4. Semantic analysis only needs to verify the shape name is known/visible

## Rationale

- Keeping shapes parallel to structs in the symbol table minimizes code duplication
- Cross-module shape resolution follows the established pattern of `global_enums` and `global_type_sigs`
- Semantic analysis checks visibility; type checking checks structural satisfaction
