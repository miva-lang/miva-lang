# 02 — Lexer and Parser for Shapes

**What to build:** The parser can now recognize `shape` keyword and parse shape definitions. Users can write `MyShape = shape { name: string, age: int, }` and it produces a valid `DShape` AST node.

**Blocked by:** 01 — Shape AST Foundation

**Status:** ready-for-agent

- [ ] Add `Token::Shape` variant to `Token` enum in `miva-frontend-rs/src/lexer.rs`
- [ ] Add `"shape" => Token::Shape` keyword mapping in lexer
- [ ] Add lexer test: `"shape"` → `Token::Shape`
- [ ] Add `parse_shape_body()` method to `Parser` — mirrors `parse_struct_body()`, parses `{ field: Type, ... }`
- [ ] Extend `parse_struct_or_func()` to check for `Token::Shape` and call `parse_shape_body()`
- [ ] Extend generic param parsing to handle bounds syntax (`T: shapeName` or `T: shape1 + shape2`) — store as `type_bounds: Vec<String>`
- [ ] Parser test: `MyShape = shape { x: int, y: int }` → `Def::DShape`
- [ ] Parser test: `hasValue[T] = shape { value: T }` → `DShape` with type_params
- [ ] Parser test: `greet[T: nameShape] = ...` → `DFunc` with type_bounds
- [ ] Run `cargo test` — no regressions
