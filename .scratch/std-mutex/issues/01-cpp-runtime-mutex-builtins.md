# 01 — C++ runtime: mvp_mutex builtins

**What to build:** Add mutex create/lock/unlock/free functions to the C++ runtime (`mvp_builtin.h`) that wrap `std::mutex`. These are the lowest-level primitives that all backends ultimately call through bridge functions or MVM opcodes.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] Add `mvp_mutex_new()` to `stdlib/mvp_builtin.h` — allocates a `std::mutex` on heap, returns `mvp_builtin_ptrany` handle
- [ ] Add `mvp_mutex_lock(handle)` to `stdlib/mvp_builtin.h` — calls `lock()` on the mutex, panics if handle is null
- [ ] Add `mvp_mutex_unlock(handle)` to `stdlib/mvp_builtin.h` — calls `unlock()`, panics if handle is null
- [ ] Add `mvp_mutex_free(handle)` to `stdlib/mvp_builtin.h` — deletes the mutex, panics if handle is null
- [ ] All functions use `static_cast<mvp_builtin_ptrany>` for return and `(std::mutex*)` cast for dereference
- [ ] Null pointer guard with `mvp_panic("mutex: null handle")` before any operation
