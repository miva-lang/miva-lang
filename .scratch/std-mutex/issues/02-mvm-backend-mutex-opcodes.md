# 02 — MVM backend: mutex opcodes

**What to build:** Wire the mutex builtins into the MVM bytecode interpreter so `mvm` can execute mutex operations at runtime.

**Blocked by:** 01 — C++ runtime: mvp_mutex builtins

**Status:** ready-for-agent

- [ ] Register `mutex_new`, `mutex_lock`, `mutex_unlock`, `mutex_free` in `symbol_table.rs` global builtins list with `Safety::Unsafe`
- [ ] Add type signatures for mutex functions in `typecheck.rs` builtin_return_type — `mutex_new` returns `TPtrAny`, others return `TNull`
- [ ] Add opcode indices in `mvm.rs` `builtin_indices` map (next available indices after existing builtins)
- [ ] Implement opcode handlers in `vm.rs` switch block:
  - `mutex_new`: push `Value::PtrAny` onto stack
  - `mutex_lock`: pop handle, call `mvp_mutex_lock`, push unit
  - `mutex_unlock`: pop handle, call `mvp_mutex_unlock`
  - `mutex_free`: pop handle, call `mvp_mutex_free`
- [ ] In `host.rs`, add extern declarations for the 4 C functions and implement them as host functions that call into the C++ runtime
- [ ] Update `mvp_host.h` generation if needed to expose mutex functions to the VM
