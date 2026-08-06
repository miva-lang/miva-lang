# 01-mutex-guard.md

**Title:** Add MutexGuard — RAII lock self-unlocking wrapper  
**Labels:** ready-for-agent  
**Date:** 2026-08-05  
**Status:** Draft

## Summary

为 `std.mutex` 新增 `MutexGuard` 类型和 `guard(m)` 函数，实现 C++ `std::lock_guard` 风格的自动解锁。Guard 持有 `ptrany` 句柄，注册 `op_drop`，析构时自动调用 `mutex_unlock`。

## Acceptance Criteria

- [ ] `std.mutex.MutexGuard` struct 可用，含 `handle: ptrany` 字段
- [ ] `std.mutex.guard(ref m: Mutex): MutexGuard` 函数可用，先 lock 再返回 Guard
- [ ] Guard 析构时自动调用 `mutex_unlock`（通过 `op_drop` + 编译器 drop 系统）
- [ ] Guard 不可拷贝（`ptrany` 字段使其自然 move-only，拷贝尝试报 E0001）
- [ ] 原有 `lock`/`unlock`/`free` API 保持不变
- [ ] `examples/mutex-guard/` 示例程序通过编译并正确运行
- [ ] 原有 `examples/mutex/` 示例不受影响
- [ ] 三个后端（cxx / llvm / mvm）均可正常工作，无需改动

## Scope

仅修改 `stdlib/std-0.1.3/src/mutex.miva`，不改动任何编译器后端代码。

## Dependencies

- `.scratch/std-mutex/` 中的 Mutex struct 和 C++ builtins（已完成）
