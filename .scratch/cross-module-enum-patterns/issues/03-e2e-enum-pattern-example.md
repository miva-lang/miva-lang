# 03 — E2E: cross-module enum pattern matching example

**What to build:** An end-to-end example demonstrating that `choose` / `when` enum pattern matching works across module boundaries with imported enums, on all three backends. The example exercises `Option.Some(v)` destructuring, `Option.None` matching, and ensures the compiler resolves variant tags and binding types correctly through the full pipeline (parser → typecheck → codegen → runtime).

**Blocked by:** #01 (parser must produce `EEnumPattern` for qualified receivers), #02 (typechecker must resolve imported enum variants)

**Status:** ready-for-agent

- [ ] Create `examples/option-pattern/` with a `main.miva` that imports `std/option` and uses `choose (x) { when (Option.Some(v)) { ... } when (Option.None) { ... } }` with a cross-module enum
- [ ] Compile and run with `-b cxx`: output matches expected values
- [ ] Compile and run with `-b llvm`: output matches expected values
- [ ] Compile and run with `-b mvm`: output matches expected values
- [ ] All three backends produce identical output
