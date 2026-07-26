# 02 — Typechecker: merge imported enums into enum-pattern lookup

**What to build:** The build system collects enum definitions from all modules in the project into a `global_enums` map (mirroring the existing `global_type_sigs` pattern). The typechecker's `check_program_with` accepts this map, merges it with the locally-built `enums` map, and uses the merged result for enum-pattern variant lookup and binding registration. After this ticket, a module that imports an enum from another module can use that enum in `choose` / `when` patterns and the typechecker resolves the variant and payload types correctly.

**Blocked by:** None — can start immediately (parallel with #01).

**Status:** ready-for-agent

- [ ] Build pre-pass (`commands/build.rs`) collects `DEnum` definitions from all source files into a `global_enums` map, keyed by both bare name and module-qualified name (mirroring `global_type_sigs` qualification logic)
- [ ] `check_program_with` signature extended to accept `global_enums: &HashMap<String, Vec<EnumVariant>>`
- [ ] Local `enums` map merged with `global_enums` inside `check_program_with` (local definitions take precedence on collision)
- [ ] Enum-pattern binding registration at `typecheck.rs:888` now succeeds for imported enums
- [ ] Typecheck test: a module importing an enum from another module uses it in a `choose`/`when` pattern; bindings are registered with correct payload types
- [ ] Existing same-module enum pattern tests still pass
