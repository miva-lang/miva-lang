# Shape System Design — `shapeName = shape { field1: Type, ... }`

Date: 2026-07-26

## Goal

Add structural shape system to Miva. A shape defines a contract of named fields with types. Any struct whose fields are a superset of a shape's fields (same names, compatible types) implicitly satisfies that shape — no explicit declaration needed, like Go interfaces. Shapes are primarily used to constrain generic type parameters, like Rust traits as bounds.

Implemented end-to-end: lexer → parser → AST → symbol table → semantic analysis → typecheck → three backends (C++ `cxx.rs`, LLVM `llvm.rs`, MVM `mvm.rs`). Zero runtime code from shapes across all backends.

## 1. Syntax

- New keyword `shape`.
- Top-level definition: `Ident [TypeParams] = shape { FieldDef, ... }`.
  - Fields use the same syntax as struct fields: `name: Type`, separated by commas, trailing comma allowed.
  - Shapes can have generic type parameters: `hasValue[T] = shape { value: T }`.
- First-class type usage: `let x: shapeName = value;` — the compiler verifies the RHS type satisfies the shape.
- Generic function bounds: `funcName[T: shapeName] = ...` or `funcName[T: shape1 + shape2] = ...` for multiple bounds.
- Bounds on generic shape params: `useValue[V: hasValue[int]] = ...`.

Example:

```miva
nameShape = shape {
  name: string,
  age: int,
}

hasValue[T] = shape {
  value: T,
}

greet[T: nameShape] = (p: T): string => p.name
multiBound[T: nameShape + ageShape] = (p: T) => p.name + " is " + string_from(p.age)
useValue[V: hasValue[int]] = (x: V): int => x.value

// Using shape as first-class type
let p: nameShape = Person { name = "Alice", age = 30, email = "alice@example.com" };
```

## 2. AST changes

**Frontend AST** (`miva-frontend-rs/src/ast.rs`) and **Compiler AST** (`miva/src/ast.rs`) — both must stay in sync:

### New Def variant: `DShape`

```rust
#[serde(rename = "shape")]
DShape {
    loc: Loc,
    name: String,
    fields: Vec<FieldDef>,
    #[serde(default)]
    type_params: Vec<String>,
}
```

Structurally parallel to `DStruct` but with serde tag `"shape"`. Uses the same `FieldDef { name, typ }`.

### New Typ variant: `TShape`

```rust
#[serde(rename = "shape")]
TShape {
    name: String,
}
```

Used when a shape is referenced as a type. Unlike `TStruct` which carries inline field data, `TShape` only stores the name — field data comes from the shape registry.

### Extended DFunc: type_bounds

Add to `DFunc` in both AST files:

```rust
#[serde(default)]
type_bounds: Vec<(String, Vec<String>)>,
```

Maps each type parameter name to its list of bound shape names. For `greet[T: nameShape + ageShape]`, this becomes `[("T", vec!["nameShape", "ageShape"])]`. For `identity[T]` (no bounds), `type_bounds` is empty.

Note: serde tuple serialization `(String, Vec<String>)` requires either a custom serializer or using a wrapper struct. Given Miva's JSON AST convention, we'll use a flat representation: `type_bounds` is a `Vec<ShapeBound>` where `ShapeBound` is a helper struct with `param_name` and `bounds` fields, serialized with a custom tag.

Alternative simpler approach: encode as `Vec<String>` where each entry is `"T:nameShape"` or `"T:nameShape+ageShape"`, parsed during semantic analysis. This avoids serde complexity and keeps the AST minimal.

**Decision:** Use `Vec<String>` with a delimited format:
```rust
#[serde(default)]
type_bounds: Vec<String>,  // e.g., "T:nameShape" or "T:nameShape+ageShape"
```

Parsed into `(param_name, [bound_names])` during type checking setup.

## 3. Lexer and Parser

### Lexer (`miva-frontend-rs/src/lexer.rs`)

- Add `Token::Shape` to the `Token` enum alongside `Token::Struct` and `Token::Enum`.
- In the keyword match block (~line 652), add: `"shape" => Token::Shape`.

### Parser (`miva-frontend-rs/src/parser.rs`)

- In `parse_struct_or_func()` (~line 183-190), after checking for `struct` and `enum`, add:
  ```rust
  if self.peek_token()? == Some(&Token::Shape) {
      return self.parse_shape_body(name, type_params, start);
  }
  ```
- New method `parse_shape_body()` — mirrors `parse_struct_body()` exactly. Parses `{ field: Type, ... }` and returns `Def::DShape`.
- Extend generic param parsing in `parse_struct_or_func()` to handle bounds:
  - After collecting bare type param names, peek for `:` token
  - If present, parse remaining input as bound specs: `shapeName` or `shape1 + shape2`
  - Collect into `type_bounds: Vec<String>` in format `"T:shape1+shape2"`
  - If no `:`, `type_bounds` remains empty (backward-compatible)

## 4. Symbol Table

`miva/src/symbol_table.rs`:

### New struct: `ShapeEntry`

```rust
pub struct ShapeEntry {
    pub name: String,
    pub fields: Vec<FieldDef>,
    pub type_params: Vec<String>,
}
```

Parallel to `StructEntry`.

### SymbolTable extensions

Add fields:
- `shapes: Vec<ShapeEntry>`
- `shape_index: HashMap<String, usize>`
- `exported_shapes: Vec<String>`

Add methods:
- `register_shape(name, type_params, fields, loc, errors)` — mirrors `register_struct`
- `lookup_shape(name)` — mirrors `lookup_struct`

Handle in `build_with_errors()`:
- `Def::DShape` → call `register_shape()`
- `SExport { symbol }` → check `shape_index.contains_key(symbol)` and add to `exported_shapes`

## 5. Semantic Analysis

`miva/src/semantic.rs`:

- When encountering `SLetTyped { typ: TShape{name}, expr }`:
  - Look up shape name in symbol table (local scope)
  - If not found locally, it may be cross-module — defer to type checking
  - No structural check here; semantic analysis only verifies the shape name is visible

- Safety checks: shapes don't affect safety levels (safe/unsafe/trusted). A safe function can accept a shape-typed parameter.

## 6. Type Checking

`miva/src/typecheck.rs` — the core of shape logic.

### Shape registry

Add `build_shape_map()` mirroring `build_struct_map()`:

```rust
fn build_shape_map(
    defs: &[Def],
) -> (HashMap<String, Vec<FieldDef>>, HashMap<String, Vec<String>>) {
    // Returns (shape_name -> fields, shape_name -> type_params)
}
```

Extend `check_program_with()` signature to accept:
- `shapes: &HashMap<String, Vec<FieldDef>>`
- `shape_type_params: &HashMap<String, Vec<String>>`
- `func_type_bounds: &HashMap<String, Vec<(String, Vec<String>)>>` (parsed from `DFunc.type_bounds`)

### TShape handling in type utilities

- `normalize_typ(TShape{name})` → pass through unchanged (no nested params to normalize)
- `resolve_type(TShape{name}, subst)` → pass through unchanged
- `types_equal(TShape{name1}, TShape{name2})` → `name1 == name2`
- `types_equal(TShape{name}, other)` → false (shapes only equal other shapes)

### Structural satisfaction check

Core function:

```
fn satisfies_shape(struct_fields: HashMap<&str, &Typ>,
                   shape_fields: &[FieldDef],
                   subst: HashMap<String, Typ>) -> bool
```

For each field in the shape:
1. Resolve the shape field's type using the substitution map (for generic shapes)
2. Look up the same field name in the struct's field map
3. If missing → return false
4. If present, compare resolved shape type with struct field type using `types_equal()`
5. If mismatch → return false
6. If all shape fields found with matching types → return true

Superset is allowed: struct can have extra fields not in the shape.

### SLetTyped with TShape

When checking `let x: someShape = expr;`:
1. Infer expression type
2. If expression type is `TStruct { name, ... }`:
   - Look up struct fields from `structs` map
   - Look up shape fields from `shapes` map
   - Call `satisfies_shape()`
   - On failure, emit E0028: `"type '{struct_name}' does not satisfy shape '{shape_name}': missing field '{field}'"`
3. If expression type is not a struct (e.g., primitive, array, ptr) → error: cannot assign non-struct to shape type
4. If expression type is itself a `TShape` → error: shape cannot be assigned to shape (shapes are contracts, not values)

### Generic function bound checking

When type-checking a call to a function with shape bounds:
1. Resolve type parameters to concrete types using substitution
2. For each resolved type parameter that has bounds:
   - Get the concrete type (should be `TStruct`)
   - Look up struct fields and shape fields
   - Call `satisfies_shape()` with the resolution substitution
   - On failure, emit E0029: `"type '{concrete_type}' does not satisfy bound '{shape_name}'"`
   - If field mismatch, emit E0030 with field-level detail

### Handling generic shape instantiation

For `V: hasValue[int]` where `hasValue[T] = shape { value: T }`:
1. Parse the bound `hasValue[int]` — shape name with type arguments
2. Look up `hasValue`'s fields: `[FieldDef { name: "value", typ: TGenericParam { name: "T" } }]`
3. Build substitution: `{ "T" → int }`
4. Resolve shape fields: `[{ name: "value", typ: TInt }]`
5. Check that the concrete type has a `value` field of type `int`

### Error codes

- E0027: unknown shape reference
- E0028: struct does not satisfy shape (missing field)
- E0029: generic bound not satisfied (type mismatch at bound level)
- E0030: generic bound field-level mismatch (with expected vs actual types)

## 7. Cross-Module Resolution

`miva/src/commands/build.rs`:

In Phase 0.5 (around line 676), alongside `global_enums`, collect `global_shapes`:

```rust
let mut global_shapes: HashMap<String, Vec<FieldDef>> = HashMap::new();
let mut global_shape_type_params: HashMap<String, Vec<String>> = HashMap::new();
```

For each `DShape` in a file's defs:
- Qualify the name using the same convention as functions (e.g., `mvp_std.io`, `main.y`)
- Insert into `global_shapes` and `global_shape_type_params`

Pass these maps into per-file `check_program_with()` and merge with local shapes (local takes precedence, same pattern as `global_enums`).

## 8. Codegen — Zero Runtime Code

Shapes produce zero output in all three backends. They are compile-time-only constructs.

### C++ backend (`miva/src/codegen/cxx.rs`)

- In `cxx_def()`, add match arm for `Def::DShape` → return `""` (empty string, skip emission)
- In `generate_header()`, shapes are not exported to headers
- `cxx_type()` must never receive `TShape` — it should be fully erased during type checking. If reached, it indicates a bug.

### CXX IR backend (`miva/src/codegen/cxx_ir.rs`)

- In `lower_defs()`, skip `Def::DShape` — no `IrDef` variant for shapes
- In `generate_header_ir()`, shapes don't appear in header declarations

### LLVM backend (`miva/src/codegen/llvm.rs`)

- Skip `DShape` definitions — no LLVM type or value emission

### MVM backend (`miva/src/codegen/mvm.rs`)

- Skip `DShape` definitions — no MVM bytecode emission

### Erasure guarantee

After type checking completes, all `TShape` references should have been resolved to their underlying concrete `TStruct` types. The `annotate_lambda_captures()` pass and subsequent codegen should never see `TShape`. If they do, it's a type checker bug.

## 9. Tests

### Lexer tests (`miva-frontend-rs/src/lexer.rs`)
- `test_shape_keyword`: `"shape"` → `Token::Shape`
- Verify shape parses in context: `MyShape = shape { x: int }` tokenizes correctly

### Parser tests (`miva-frontend-rs/src/parser.rs`)
- `test_parse_shape`: `Color = shape { r: int, g: int, b: int }` → `Def::DShape`
- `test_parse_generic_shape`: `hasValue[T] = shape { value: T }` → `DShape` with `type_params`
- `test_parse_shape_bounds`: `greet[T: nameShape] = ...` → `DFunc` with `type_bounds`
- `test_parse_multi_bounds`: `f[T: shape1 + shape2] = ...` → `type_bounds` with two entries

### Symbol table tests (`miva/src/symbol_table.rs`)
- Shape registration and lookup
- Duplicate shape detection (E0004)
- Export tracking for shapes

### Type checker tests (`miva/src/typecheck.rs`)
- Exact field match: struct with exactly the shape's fields → OK
- Superset: struct with extra fields → OK
- Missing field: struct lacks a required field → E0028
- Type mismatch: field exists but wrong type → E0028
- Generic shape: `hasValue[int]` satisfied by struct with `value: int`
- Generic shape: `hasValue[int]` NOT satisfied by struct with `value: string` → E0030
- Single bound: `T: nameShape` satisfied → OK
- Multiple bounds: `T: nameShape + ageShape` satisfied → OK
- Multiple bounds: `T: nameShape + ageShape` fails one bound → E0029
- Non-struct assigned to shape → error
- Cross-module shape satisfaction

### Integration test
- Complete program: define shapes, structs, generic functions with bounds, use shapes as type annotations
- Builds and runs on all three backends (cxx, llvm, mvm)
- Verifies correct compilation output (no shape-related symbols in generated code)

## Scope / Non-goals

- No method definitions inside shapes (shapes describe data fields only, not behavior)
- No trait-like default implementations
- No trait objects / dynamic dispatch (shapes are purely static/compile-time)
- No shape inheritance (shapes don't extend other shapes; use multiple bounds with `+` instead)
- No runtime type information for shapes (no `is SomeShape` checks)
- No shape polymorphism in enums (enums remain ADTs, not shape-based)
- No subtyping between shapes themselves (a shape is not a subtype of another shape unless explicitly checked via bounds)
