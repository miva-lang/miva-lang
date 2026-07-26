# 01 — Parser: recognize module-qualified enum pattern receivers

**What to build:** The frontend parser accepts module-qualified receivers like `std.option.Option.Some(v)` in `choose` / `when` patterns and produces an `EEnumPattern` node with the correct `enum_name`. Previously only bare uppercase identifiers like `Option.Some(v)` were converted; qualified receivers stayed as `EMethodCall` and never reached the enum-pattern code path.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] `method_call_or_pattern` detects `EFieldAccess` chains whose leaf identifier starts with an uppercase letter (e.g. `std.option.Option`)
- [ ] Extracts the last segment (`"Option"`) as the `enum_name` stored in `EEnumPattern`
- [ ] Variant name (`method`) and bindings (`args`) are unchanged from the existing bare-receiver case
- [ ] Explicit type arguments (`Option.Some[int](v)`) still stay as `EMethodCall` (constructor call, not pattern)
- [ ] Parser unit test: `std.option.Option.Some(v)` parses as `EEnumPattern { enum_name: "Option", variant: "Some", bindings: ["v"] }`
- [ ] Existing bare-receiver test `Option.Some(v)` still passes
