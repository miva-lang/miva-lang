# Implementation Plan: Shape System for Miva

## Overview

Add structural shape system to Miva — a Rust-trait-like, Go-interface-like mechanism for constraining generic type parameters. Shapes are compile-time only with zero runtime cost across all three backends (cxx, llvm, mvm).

## Phase 1: AST Foundation (miva-frontend-rs + miva/src/ast.rs)

**Files:** `miva-frontend-rs/src/ast.rs`, `miva/src/ast.rs`

1. Add `DShape` variant to `Def` enum in both files (must stay in sync):
   ```rust
   #[serde(rename = "shape")]
   DShape { loc: Loc, name: String, fields: Vec<FieldDef>, #[serde(default)] type_params: Vec<String> }
   ```

2. Add `TShape` variant to `Typ` enum in both files:
   ```rust
   #[serde(rename = "shape")]
   TShape { name: String }
   ```

## Phase 2: Lexer and Parser (miva-frontend-rs/src/)

**Files:** `lexer.rs`, `parser.rs`

3. Add `Token::Shape` to `Token` enum in `lexer.rs`

4. Add `"shape" => Token::Shape` to keyword match in `lexer.rs` (~line 652)

5. Add `parse_shape_body()` method to `Parser` — mirrors `parse_struct_body()` exactly, returns `Def::DShape`

6. In `parse_struct_or_func()`, add branch after `enum` check:
   ```rust
   if self.peek_token()? == Some(&Token::Shape) {
       return self.parse_shape_body(name, type_params, start);
   }
   ```

7. Add `type_bounds: Vec<(String, Vec<String>)>` field to `DFunc` in frontend AST

8. Extend generic param parsing in `parse_struct_or_func()` to handle bounds syntax:
   - After collecting type param names, peek for `:` 
   - If present, parse bound spec: `shapeName` or `shape1 + shape2`
   - Store as `(param_name, vec_of_bound_names)` in the new field

9. Add test for shape parsing: `test_parse_shape()`

## Phase 3: Symbol Table (miva/src/symbol_table.rs)

**File:** `symbol_table.rs`

10. Add `ShapeEntry` struct parallel to `StructEntry`:
    ```rust
    pub struct ShapeEntry {
        pub name: String,
        pub fields: Vec<FieldDef>,
        pub type_params: Vec<String>,
    }
    ```

11. Add to `SymbolTable`:
    - `shapes: Vec<ShapeEntry>`
    - `shape_index: HashMap<String, usize>`
    - `exported_shapes: Vec<String>`

12. Add methods:
    - `register_shape()` — mirrors `register_struct()`
    - `lookup_shape()` — mirrors `lookup_struct()`

13. Handle `Def::DShape` in `build_with_errors()` — call `register_shape()`

14. Handle exported shapes in `SExport` arm — add to `exported_shapes`

## Phase 4: Semantic Analysis (miva/src/semantic.rs)

**File:** `semantic.rs`

15. In `check_expr()`, when encountering a `SLetTyped` with `TShape` type:
    - Look up shape name in symbol table
    - If not found locally, it may be cross-module (deferred to type checking)
    - Verify shape name is known

16. In `check_expr()` for `ECall` with generic function that has shape bounds:
    - After type inference resolves concrete types, verify satisfaction

## Phase 5: Type Checking (miva/src/typecheck.rs)

**File:** `typecheck.rs`

17. Add `build_shape_map()` function — mirrors `build_struct_map()`:
    ```rust
    fn build_shape_map(defs: &[Def]) 
        -> (HashMap<String, Vec<FieldDef>>, HashMap<String, Vec<String>>)
    ```

18. Extend `check_program_with()` signature to accept:
    - `shapes: &HashMap<String, Vec<FieldDef>>`
    - `shape_type_params: &HashMap<String, Vec<String>>`
    - `func_type_bounds: &HashMap<String, Vec<(String, Vec<String>)>>`

19. Handle `TShape` in `Typ` enum variants:
    - `normalize_typ()`: pass through unchanged
    - `resolve_type()`: pass through unchanged  
    - `types_equal()`: compare by name

20. Implement `satisfies_shape()` helper:
    - Takes struct fields (as HashMap), shape fields, and type substitution map
    - Returns bool indicating structural compatibility
    - For each shape field, look up matching struct field by name
    - Compare types using `types_equal()` with resolution

21. Handle `SLetTyped { typ: TShape{name}, expr }`:
    - Infer expression type
    - If it's a `TStruct`, look up struct fields
    - Call `satisfies_shape()` with substitution-resolved shape fields
    - Emit E0028/E0030 error on mismatch

22. Handle generic function bounds in `check_program_with()`:
    - For each func with `type_bounds`, after resolving type params:
    - For each resolved concrete type, verify it satisfies all bound shapes
    - Emit E0029/E0030 on mismatch

23. Handle `EStructLit` with shape context — when struct literal is assigned to shape-typed variable, verify fields match

## Phase 6: Cross-Module Resolution (miva/src/commands/build.rs)

**File:** `build.rs`

24. In Phase 0.5, add `global_shapes` and `global_shape_type_params` collection:
    ```rust
    let mut global_shapes: HashMap<String, Vec<FieldDef>> = HashMap::new();
    let mut global_shape_type_params: HashMap<String, Vec<String>> = HashMap::new();
    ```

25. For each `DShape` in defs, qualify name and insert into maps (same pattern as `global_enums`)

26. Pass `global_shapes` and `global_shape_type_params` to per-file `check_program_with()`

27. Merge cross-module shapes into per-file check (same pattern as `global_enums` merge)

## Phase 7: Codegen — Zero Output (all backends)

**Files:** `miva/src/codegen/cxx.rs`, `cxx_ir.rs`, `llvm.rs`, `mvm.rs`

28. **cxx.rs**: In `cxx_def()`, add match arm for `Def::DShape` → return empty string

29. **cxx.rs**: In `generate_header()`, skip shapes (no header declarations)

30. **cxx_ir.rs**: In `lower_defs()`, skip `IrDef::Shape` (or just don't create one)

31. **cxx_ir.rs**: In `generate_header_ir()`, skip shapes

32. **llvm.rs**: Skip `DShape` in codegen — no LLVM type emission

33. **mvm.rs**: Skip `DShape` in bytecode generation — no opcodes

34. Ensure `TShape` never reaches any backend — it should be fully erased during type checking. If it does, panic with clear error.

## Phase 8: Tests

35. Add parser tests for shape definitions and bounds syntax

36. Add type checker tests:
    - Shape satisfaction (exact match, superset, missing field, wrong type)
    - Generic bounds (single bound, multiple bounds with +)
    - Generic shape instantiation (`hasValue[int]`)
    - Cross-module shape satisfaction

37. Add integration test: complete program using shapes

## File Change Summary

| File | Changes |
|------|---------|
| `miva-frontend-rs/src/ast.rs` | Add `DShape`, `TShape`, extend `DFunc` with `type_bounds` |
| `miva/src/ast.rs` | Mirror frontend AST changes |
| `miva-frontend-rs/src/lexer.rs` | Add `Token::Shape`, keyword mapping |
| `miva-frontend-rs/src/parser.rs` | Add `parse_shape_body()`, extend generic param parsing |
| `miva/src/symbol_table.rs` | Add `ShapeEntry`, shape tracking methods |
| `miva/src/semantic.rs` | Shape visibility check in `check_expr()` |
| `miva/src/typecheck.rs` | Shape satisfaction logic, bound checking, `TShape` handling |
| `miva/src/commands/build.rs` | Global shapes collection, cross-module merge |
| `miva/src/codegen/cxx.rs` | Skip shapes in codegen |
| `miva/src/codegen/cxx_ir.rs` | Skip shapes in IR lowering |
| `miva/src/codegen/llvm.rs` | Skip shapes |
| `miva/src/codegen/mvm.rs` | Skip shapes |

## Execution Order

Do phases in order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8

Each phase should compile successfully before proceeding to the next. Run `cargo test` after each phase.
