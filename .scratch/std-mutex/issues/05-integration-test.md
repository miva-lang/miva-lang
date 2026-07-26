# 05 — Integration: end-to-end mutex test

**What to build:** A complete Miva program that demonstrates mutex usage across async tasks, verifying the full stack works end-to-end.

**Blocked by:** 02 — MVM backend: mutex opcodes, 03 — LLVM/CXX backends: mutex bridge and codegen

**Status:** ready-for-agent

- [ ] Create `examples/mutex/src/main.miva` demonstrating:
  - Import `std/mutex`
  - Create a shared counter and a mutex
  - Spawn multiple async workers that lock/unlock around counter access
  - Await all workers and print final counter value
- [ ] Verify the program compiles and runs correctly on all three backends (cxx, llvm, mvm)
- [ ] Verify output shows correct sequential accumulation (no data races)
- [ ] Test edge case: double-lock should panic or deadlock (std::mutex is not reentrant)
