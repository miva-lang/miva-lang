# ADR-003: Shape Parsing and Lexing

**Status:** Proposed  
**Date:** 2026-07-26

## Context

The frontend parser (`miva-frontend-rs/src/parser.rs`) handles `struct` and `enum` definitions in `parse_struct_or_func()`. The lexer (`miva-frontend-rs/src/lexer.rs`) maps keyword strings to tokens.

## Decision

### Lexer changes

In `lexer.rs`, line ~652, add:

```rust
"shape" => Token::Shape,
```

Add `Token::Shape` to the `Token` enum alongside `Token::Struct` and `Token::Enum`.

### Parser changes

In `parser.rs`, `parse_struct_or_func()` (line ~183-190), after checking for `struct` and `enum`, add a third branch:

```rust
// Check for shape
if self.peek_token()? == Some(&Token::Shape) {
    return self.parse_shape_body(name, type_params, start);
}
```

New method `parse_shape_body()` mirrors `parse_struct_body()`:

```rust
fn parse_shape_body(
    &mut self,
    name: String,
    type_params: Vec<String>,
    start: usize,
) -> Result<Def, String> {
    self.advance()?; // consume "shape"
    self.expect(&Token::LBrace)?;
    let mut fields = Vec::new();
    while self.peek_token()? != Some(&Token::RBrace) {
        let (field_name, _) = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        let typ = self.parse_typ()?;
        fields.push(FieldDef { name: field_name, typ });
        if self.peek_token()? == Some(&Token::Comma) {
            self.advance()?;
        }
    }
    self.expect(&Token::RBrace)?;
    Ok(Def::DShape {
        loc: self.loc(start),
        name,
        fields,
        type_params,
    })
}
```

### Generic bounds parsing on functions

For `identity[T: nameShape]`, the generic parameter syntax needs extension. Current `parse_struct_or_func()` parses `[T, U]` as bare identifiers. We need to extend this to optionally parse bounds:

After collecting all type param names, check for `:` and parse bound specs. The grammar for bounds:

```
[TypeParams] where TypeParams : BoundSpecs
```

But since Miva uses Rust-like inline bounds, we parse it as:

```
name[T] or name[T: ShapeBound]
```

Where `ShapeBound` is one of:
- `T: shapeName` — single bound
- `T: shape1 + shape2` — multiple bounds

Implementation approach: After parsing the identifier name in `parse_struct_or_func()`, check for `[`. If present, parse type params with optional bounds:

```
parse_generic_params() -> Vec<(String, Vec<String>)>
// returns list of (param_name, [bound_shapes])
```

This replaces the current `Vec<String>` type_params. But that would be a breaking change to many places. Alternative: keep `type_params: Vec<String>` in the AST and add a new `type_bounds: Vec<(String, Vec<String>)>` field to `DFunc`.

**Decision:** Add `type_bounds: Vec<(String, Vec<String>)>` to `DFunc` in both AST files. This is backward-compatible — existing code without bounds has an empty `type_bounds`.

## Rationale

- Mirroring struct parsing for shapes minimizes new code
- Separate `type_bounds` field avoids changing the generic param parsing contract
- Rust-style `+` for multiple bounds is familiar to systems programmers
