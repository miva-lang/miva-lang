# ADR-006: Shape Codegen — Zero Runtime Cost

**Status:** Proposed  
**Date:** 2026-07-26

## Context

Miva has three backends: C++ (`cxx.rs`), LLVM IR (`llvm.rs`), and MVM bytecode (`mvm.rs`). Shapes are compile-time constraints with no runtime representation.

## Decision

### No shape code generation

In all three backends, `DShape` definitions are silently skipped during codegen. They produce no C++ struct, no LLVM type, no MVM opcode.

### TShape erasure

The `TShape` type variant must never reach codegen. During type checking, when a variable is declared as `TShape`, its type is resolved to the underlying concrete struct type. The shape constraint is verified and then discarded.

Example transformation:

```miva
let p: nameShape = person;
```

After type checking, `p`'s type is `TStruct { name: "Person", ... }`, not `TShape { name: "nameShape" }`.

### Backend-specific handling

**C++ backend (`cxx.rs`):**
- `cxx_def()` matches on `Def::DShape` → returns empty string (skip)
- `cxx_type()` for `Typ::TShape` → should never be reached; if it is, panic or return error
- Exported shapes don't generate header declarations

**LLVM backend (`llvm.rs`):**
- Same pattern — skip `DShape`, panic on unexpected `TShape`

**MVM backend (`mvm.rs`):**
- Same pattern — skip `DShape`, panic on unexpected `TShape`

**CXX IR backend (`cxx_ir.rs`):**
- `IrDef` has no shape variant
- `lower_defs()` skips `DShape`
- `emit_struct_def()` not called for shapes

## Rationale

- Shapes are purely a compile-time concept — no runtime overhead
- Erasing `TShape` to concrete types during type checking simplifies all backends
- Panic-on-unreachable is consistent with Miva's existing approach (e.g., `EMethodCall` and `EMacroVar` hit `unreachable!()` in some code paths)
