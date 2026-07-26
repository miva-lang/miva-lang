# Cross-Module Enum Pattern Matching

**Date:** 2026-07-24  
**Status:** Proposed

## Problem Statement

Miva's `choose` / `when` enum pattern matching works for enums defined in the same module, but fails silently or errors when matching against enums imported from another module.

Specifically, two issues prevent cross-module enum destructuring:

1. **Parser gap**: `method_call_or_pattern` only converts `Enum.Variant(x, y)` into an `EEnumPattern` when the receiver is a bare `EVar` starting with an uppercase letter. Module-qualified receivers like `std.option.Option.Some(v)` remain as `EMethodCall`, so they never reach the enum-pattern code path.

2. **Typechecker gap**: `build_enum_maps` only indexes enums defined in the current module's `defs`. When the typechecker processes a pattern like `Option.Some(v)` in a file that imported `Option` from another module, the lookup `enums.get("Option")` returns `None` because the map contains only local definitions. The pattern's bindings are never registered, leading to "undefined variable" errors downstream.

This means users cannot write idiomatic enum pattern matching across module boundaries:

```miva
import "std/option";
let x std.option.Option[int] = std.option.some[int](42);
choose (x) {
    when (Option.Some(v)) { printlns!("value = ", v); }
    when (Option.None)    { printlns!("empty"); }
}
```

The workaround today is to use helper functions (`is_some`, `unwrap`, etc.) instead of direct pattern matching, which is less ergonomic and inconsistent with same-module usage.

## Solution

Enable `choose` / `when` enum pattern matching against enums defined in any module, using the same syntax whether the enum is local or imported.

### User Stories

1. As a Miva programmer, I want to write `when (Option.Some(v))` in a module that imported `Option` from another module, so that I can destructure enum values without calling helper functions.
2. As a Miva programmer, I want to write `when (std.option.Option.Some(v))` with a fully-qualified enum path, so that I can disambiguate when two imported modules define enums with the same name.
3. As a Miva programmer, I want the compiler to resolve the correct enum variants and register pattern bindings with their payload types, so that the `when` branch body type-checks correctly.
4. As a Miva programmer, I want this to work consistently across all three backends (cxx, llvm, mvm), so that I can use enum patterns regardless of my compilation target.

## Implementation Decisions

### Parser: Accept module-qualified enum pattern receivers

The `method_call_or_pattern` function in the frontend parser currently only matches `Enum.Variant(args)` when the receiver is a bare `EVar`. It needs to also accept `EFieldAccess` chains whose last segment starts with an uppercase letter (e.g. `std.option.Option`).

When a qualified receiver is detected, the parser extracts the **last segment** (e.g. `"Option"` from `std.option.Option`) as the `enum_name` stored in `EEnumPattern`. The variant name (`method`) and bindings (`args`) are unchanged.

This means:
- `Option.Some(v)` → `EEnumPattern { enum_name: "Option", variant: "Some", bindings: ["v"] }` (unchanged)
- `std.option.Option.Some(v)` → `EEnumPattern { enum_name: "Option", variant: "Some", bindings: ["v"] }` (new)
- `std.option.Option.Some[int](v)` → stays as `EMethodCall` because `type_args` is non-empty (constructor call, not pattern)

The `EFieldAccess` receiver case is detected by walking the receiver chain to find the last `EFieldAccess` or `EVar` node and extracting its leaf identifier. If that identifier starts with an uppercase letter and all args are bare identifiers, the conversion to `EEnumPattern` proceeds.

### Build system: Collect cross-module enum definitions

The build system (`commands/build.rs`) already collects cross-module function signatures into `global_type_sigs` during a pre-pass over all source files. An analogous `global_enums` map will be collected in the same pass.

Each file's module name is determined by its `DModule` declaration (or inferred from its path). For each `DEnum` definition found during the pre-pass, two entries are stored:

- **Bare key**: the enum's declared name (e.g. `"Option"`)
- **Qualified key**: `<module_prefix>.<enum_name>` (e.g. `"std.option.Option"` or `"mvp_std.option.Option"`)

The qualified key mirrors the existing function-signature qualification logic (`util::process_call_path`).

### Typechecker: Merge local and imported enum maps

`check_program_with` will accept a new parameter: `global_enums: &HashMap<String, Vec<EnumVariant>>`.

Inside `check_program_with`, the local `enums` map (from `build_enum_maps`) is merged with `global_enums`. Local definitions take precedence on key collision (same as `global_type_sigs`). The merged map is then passed to `infer_type` / `require_type` as before.

This means the existing enum-pattern lookup at `typecheck.rs:888` (`enums.get(enum_name.as_str())`) will succeed for both local and imported enums, because the merged map contains both.

### Enum type resolution in patterns

When the pattern's `enum_name` is resolved to a variant, the existing type parameter substitution logic applies: the variant's payload types are normalized against the enum's `type_params` and substituted with the scrutinee's concrete `type_args`. This logic is unchanged; it now simply has access to imported enums.

### Codegen: No changes required

The cxx, llvm, and mvm backends already handle `EEnumPattern` correctly. They use `enum_name` to look up the variant's tag and emit the appropriate comparison / destructuring code. Since the parser now produces `EEnumPattern` for qualified receivers and the typechecker resolves the enum name to a variant, no codegen changes are needed.

## Testing Decisions

- Add a parser test: `std.option.Option.Some(v)` is parsed as `EEnumPattern` with `enum_name: "Option"`.
- Add a typecheck test: a module that imports an enum from another module and uses it in a `choose` / `when` pattern; verify that bindings are registered with the correct payload types.
- Add an end-to-end example: `examples/option-pattern/` demonstrating `choose (x) { when (Option.Some(v)) { ... } when (Option.None) { ... } }` with a cross-module enum.
- Verify all three backends (cxx, llvm, mvm) compile and run the example with identical output.

## Out of Scope

- Short-circuit / ambiguity resolution when two imported modules define enums with the same bare name and both are used in patterns without qualification. The current design uses basename matching (same as `types_equal`), which mirrors existing cross-module struct/enum behavior.
- Pattern matching on generic enum constructors with explicit type arguments (e.g. `Option.Some[int](v)`). This remains a constructor call, not a pattern, consistent with current parser behavior.
- IDE / LSP support for cross-module enum navigation.

## Further Notes

- The `types_equal` function already performs basename matching for cross-module type equality, so the enum pattern fix is consistent with existing cross-module type resolution.
- The `build_enum_maps` function is unchanged for local-only use cases; the merge happens in `check_program_with`, mirroring how `global_type_sigs` is merged with local `func_sigs`.
- The parser change is backward-compatible: existing `Option.Some(v)` patterns (bare receiver) continue to work identically.
