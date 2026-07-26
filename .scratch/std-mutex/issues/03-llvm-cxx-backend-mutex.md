# 03 — LLVM/CXX backends: mutex bridge and codegen

**What to build:** Wire the mutex builtins into the LLVM and CXX code generation backends so they emit correct bridge calls and C++ function mappings.

**Blocked by:** 01 — C++ runtime: mvp_mutex builtins

**Status:** ready-for-agent

- [ ] Add `miva_mutex_new`, `miva_mutex_lock`, `miva_mutex_unlock`, `miva_mutex_free` bridge functions in `llvm.rs` `generate_bridge()` — matching the pattern used for json/ptr functions (e.g., `mvp_mutex_new` returns `void*`, lock/unlock/free take `int64_t` handle)
- [ ] Add LLVM IR declarations in `llvm.rs` `decls` section for the 4 bridge functions
- [ ] Add builtin name mapping in `llvm.rs` `builtin_func_name()` match arm: `"mutex_new"` → `"@miva_mutex_new"`, etc.
- [ ] Add CXX builtin mapping in `cxx.rs` `map_builtin()`: `"mutex_new"` → `"mvp_mutex_new"`, etc.
- [ ] Register `mutex_new`, `mutex_lock`, `mutex_unlock`, `mutex_free` in `symbol_table.rs` global builtins list with `Safety::Unsafe`
- [ ] Add type signatures for mutex functions in `typecheck.rs` builtin_return_type — `mutex_new` returns `TPtrAny`, others return `TNull`
