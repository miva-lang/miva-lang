# Miva Programming Language

Miva is a Rust-inspired language with move semantics and deterministic drop.
A shared frontend lowers Miva source to three interchangeable backends that
produce byte-identical program output:

- **cxx** — transpile to C++, compile with a system C++ compiler to a native binary (default).
- **llvm** — emit LLVM IR and compile to a native binary.
- **mvm** — compile to `.mvm` bytecode, executed by the `mvm` interpreter/JIT.

## Workspace

| Crate | Description |
| --- | --- |
| `miva` | CLI compiler driver, project tooling, and the cxx/llvm backends. |
| `miva-frontend-rs` | Lexer, parser, semantic analysis, and type checking. |
| `miva-vm` | The `mvm` bytecode interpreter and JIT. |

## Build

```bash
./build.sh            # debug build of the whole workspace
./build.sh --release  # release build
./build.sh --test     # debug build + cargo test --workspace
```

This produces `target/<mode>/miva` (compiler) and `target/<mode>/mvm` (interpreter).

## Usage

```bash
miva init            # scaffold a new project (miva.toml + src/main.miva)
miva build           # compile the project
miva run             # compile and run
miva run -b llvm     # choose a backend: cxx (default) | llvm | mvm
miva test            # run project tests
miva clean           # remove build artifacts
```

Add `-r`/`--release` for optimized builds and `-v` for verbose output.
When running with the `mvm` backend, the driver locates the interpreter via the
`MIVA_MVM` environment variable, then `PATH`, then binaries beside the compiler.

## Language

### Type system

`int`, `float32`, `float64`, `bool`, `string`, `char`, `array<T>`, `ptr<T>`,
`box<T>`, `null`, `ptrany`.

### Safety levels

- `safe` — safe by default.
- `unsafe` — requires explicit declaration.
- `trusted` — bypasses safety checks in a controlled scope.

### Ownership & drop

Miva values are move-only. A droppable type is destroyed deterministically when
its owner's scope ends without the value having been moved away; the compiler
inserts an ordinary call to the drop function registered via `op_drop`. The
`drop(x)` builtin destroys a value early. See `CONTEXT.md` for the full
glossary.

### Example

```miva
// src/main.miva
module main;

add = (x: int, y: int): int => {
    return x + y;
}

main = () => {
    printlns!("10 + 20 = ", add(10, 20));
}
```

```bash
miva run            # cxx backend
miva run -b llvm    # llvm backend
miva run -b mvm     # mvm bytecode backend
```

## Architecture

```
Miva source
  → Lexer → Parser → AST
  → Symbol Table → Semantic Analysis → Type Checking → Macro Expansion
  → backend:
      cxx : C++ source → system C++ compiler → native binary
      llvm: LLVM IR      → native binary
      mvm : .mvm bytecode → mvm interpreter / JIT
```

The `examples/` directory holds runnable projects; `miva/tests/backend_parity.rs`
asserts the three backends emit identical output for representative programs.
