# 07 — Codegen: Zero Runtime Code for Shapes

**What to build:** All three backends (C++, LLVM, MVM) skip shape definitions and produce zero output. `TShape` types are fully erased during type checking and never reach codegen.

**Blocked by:** None — can start immediately (purely additive changes).

**Status:** ready-for-agent

- [ ] `cxx.rs`: In `cxx_def()`, add match arm for `Def::DShape` → return `""` (skip)
- [ ] `cxx.rs`: Ensure shapes don't appear in header declarations
- [ ] `cxx_ir.rs`: Skip `DShape` in `lower_defs()` — no `IrDef` variant
- [ ] `llvm.rs`: Skip `DShape` in codegen — no LLVM emission
- [ ] `mvm.rs`: Skip `DShape` in bytecode generation
- [ ] Verify `TShape` never reaches any backend — if it does, it's a type checker bug
- [ ] Run `cargo test` — verify no shape-related symbols in generated code
