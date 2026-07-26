# 04 — std/mutex: Miva standard library module

**What to build:** The user-facing `std/mutex` module that wraps the raw builtin functions into a friendly Miva API with a `Mutex` struct.

**Blocked by:** None — can start immediately (the module only references builtin function names; they don't need to exist at parse time).

**Status:** ready-for-agent

- [ ] Create `stdlib/std-0.1.3/src/mutex.miva` following the `std/json` pattern
- [ ] Define `Mutex = struct { handle: ptrany }`
- [ ] Implement `new()` → returns `Mutex` calling `mvp_mutex_new()`
- [ ] Implement `unsafe lock(ref m: Mutex)` → calls `mvp_mutex_lock(m.handle)`
- [ ] Implement `unsafe unlock(ref m: Mutex)` → calls `mvp_mutex_unlock(m.handle)`
- [ ] Implement `unsafe free(ref m: Mutex)` → calls `mvp_mutex_free(m.handle)`
- [ ] Export all 5 symbols: `Mutex`, `new`, `lock`, `unlock`, `free`
- [ ] Add module documentation comments explaining usage and safety
