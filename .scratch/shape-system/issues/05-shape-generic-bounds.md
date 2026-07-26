# 05 — Shape Generic Bounds on Functions

**What to build:** Generic functions can now declare shape bounds (`T: nameShape` or `T: nameShape + ageShape`). When calling such functions, the compiler verifies the concrete type satisfies all bound shapes.

**Blocked by:** 02 — Lexer and Parser for Shapes, 04 — Shape Type Checking: Satisfaction and First-Class Types

**Status:** ready-for-agent

- [ ] Parse `type_bounds` from `DFunc.type_bounds` (format: `"T:shape1+shape2"`) into `(param_name, [bound_names])` tuples
- [ ] Build `func_type_bounds` map in `check_program_with()`
- [ ] For each generic function call with bounds:
  - Resolve type parameters to concrete types
  - For each resolved type param with bounds, verify satisfaction against all bound shapes
  - Handle generic shape instantiation: `hasValue[int]` resolves shape fields with substitution
- [ ] Emit E0029 for bound not satisfied (type level)
- [ ] Emit E0030 for bound field mismatch (field level with expected vs actual)
- [ ] Run `cargo test` — verify single bound, multiple bounds, generic shape instantiation cases
