# 08 — End-to-End Integration Test

**What to build:** A complete Miva program demonstrating shape system: define shapes, structs that satisfy them, generic functions with bounds, and shape-typed variables. Builds and runs on all three backends (cxx, llvm, mvm).

**Blocked by:** 05 — Shape Generic Bounds on Functions, 06 — Cross-Module Shape Resolution, 07 — Codegen: Zero Runtime Code for Shapes

**Status:** ready-for-agent

- [ ] Create `examples/shape-system/main.miva` with:
  - Shape definitions: `nameShape`, `ageShape`, `hasValue[T]`
  - Structs satisfying shapes (exact match, superset)
  - Generic function with single bound: `greet[T: nameShape]`
  - Generic function with multiple bounds: `multiBound[T: nameShape + ageShape]`
  - Generic shape instantiation: `useValue[V: hasValue[int]]`
  - First-class shape type annotation: `let p: nameShape = person`
  - Error cases: struct missing field, wrong field type
- [ ] Build and run on C++ backend (`miva build -b cxx`)
- [ ] Build and run on LLVM backend (`miva build -b llvm`)
- [ ] Build and run on MVM backend (`miva build -b mvm`)
- [ ] Verify no shape-related symbols in generated code
- [ ] Run `cargo test` — full regression pass
