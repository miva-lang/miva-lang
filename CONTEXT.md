# Miva Language

The Miva compiler and toolchain: a Rust-inspired language with move semantics, compiled through three backends (cxx, llvm, mvm) from a shared frontend.

## Language

### Ownership & Drop

**Droppable**:
A type that requires destruction at end of ownership — either it registers `op_drop` directly, or it transitively contains a droppable field (drop glue makes droppability infectious).
_Avoid_: destructible, RAII type, resource type

**Drop**:
The deterministic destruction of a value when its owner's scope ends without the value having been moved away. Inserted by the compiler as an ordinary call to the registered drop function.
_Avoid_: destructor call, finalize, free

**Drop function**:
The user-written free function registered via `op_drop` in an `impl` block, with signature `(ref self: T) -> unit`. It may only be invoked through compiler-inserted drops or the `drop(x)` builtin — never called directly or taken as a value.
_Avoid_: destructor, finalizer, dtor

**Drop glue**:
Compiler-synthesized destruction logic for aggregates: a value's own drop function runs first, then its fields are dropped in declaration order; local variables drop in reverse declaration order; enums drop only the live variant's payload.

**Move-only**:
A type whose values cannot be implicitly copied; assignment and argument passing transfer ownership. Every droppable type is move-only. Explicit `clone` remains allowed — the cloner owns the clone's resources.
_Avoid_: non-copyable, linear type

**`drop(x)` builtin**:
The only user-facing way to destroy a value early. Takes ownership of `x` (marking it moved) and invokes its drop.
_Avoid_: manual destructor call, dispose

### Existing concepts

**Impl block**:
A `impl StructName { op_add fn_name, ... }` definition mapping operator keywords (including `op_drop`) to free functions for a struct.
_Avoid_: trait impl, extension

**Shape**:
A compile-time-only structural type (`shape { ... }`) used for structural checks and generic bounds; erased at codegen. Shapes cannot have impl blocks.
