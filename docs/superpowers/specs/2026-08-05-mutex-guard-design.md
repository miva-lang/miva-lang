# Mutex Guard — Lock Self-Unlocking RAII Wrapper

**Date:** 2026-08-05  
**Status:** Draft  
**Related:** `.scratch/std-mutex/` (Mutex struct + C++ builtins already landed)

## Problem Statement

Miva 当前的 `std.mutex` 提供了手动 `lock`/`unlock` API：

```miva
m := std.mutex.create();
std.mutex.lock(m);
// 临界区...
std.mutex.unlock(m);  // 必须手动调用，否则死锁
std.mutex.free(m);
```

这有两个问题：

1. **调用者容易忘记 unlock**——如果在临界区之后、`unlock` 之前发生 `return` 或 `panic`，锁永远不会释放，导致死锁。
2. **代码冗长**——每个临界区都必须成对写 `lock` 和 `unlock`，在有多条退出路径的代码中尤其烦人。

用户期望一种类似 C++ `std::lock_guard` 的 RAII 模式：锁在作用域入口获取，在作用域出口自动释放，无需手动干预。

## Solution

新增 `std.mutex.MutexGuard` 类型和一个新的 `std.mutex.guard(m)` 函数：

- `MutexGuard` 是一个持有 `ptrany` 句柄的 struct，注册 `op_drop`，析构时自动调用 `mutex_unlock`。
- `guard(m)` 接受 `ref m: Mutex`，先 lock，再返回一个 `MutexGuard`。
- `MutexGuard` 因含 `ptrany` 字段而自动成为 move-only（`ptrany` 不是 copy 类型），无需额外机制。
- 编译器已知的 drop 系统会在 `MutexGuard` 离开作用域时自动插入 `drop_guard(g)` 调用——零代码gen 改动。
- 原有 `lock`/`unlock` API 保留不变，向后兼容。

**用户代码对比：**

```miva
// 旧写法
m := std.mutex.create();
std.mutex.lock(m);
printlns!("critical section");
std.mutex.unlock(m);
std.mutex.free(m);

// 新写法
m := std.mutex.create();
g := std.mutex.guard(m);
printlns!("critical section");
// g 析构时自动 unlock，无需手动调用
std.mutex.free(m);
```

## User Stories

1. As a Miva 用户，我想用 `std.mutex.guard(m)` 而不是手动 `lock`/`unlock`，这样临界区代码更简洁且不会因忘记 unlock 而死锁。
2. As a Miva 用户，我希望 Guard 在作用域退出时自动解锁，即使有 `return`、`break`、`continue` 等多条退出路径，这样我不需要在每个出口都手动 unlock。
3. As a Miva 用户，我希望 `guard(m)` 不消耗 `m`（接受 `ref m`），这样同一个 Mutex 可以被多次 lock，产生多个独立的 Guard。
4. As a Miva 用户，我希望 `MutexGuard` 是 move-only 的，这样它不能被意外拷贝（拷贝两份 Guard 析构时会双解锁，导致未定义行为）。
5. As a Miva 用户，我希望现有 `std.mutex.lock`/`unlock`/`free` API 继续可用，这样已有代码无需修改。
6. As a Miva 用户，我希望 Guard 的 `op_drop` 只在锁处于已锁状态时才 unlock，这样对未 lock 的 Guard 析构不会触发 panic。
7. As a Miva 标准库维护者，我希望 Guard 的实现在 stdlib Miva 代码中，不依赖任何后端代码改动，这样三个后端（cxx/llvm/mvm）零改动即可支持。
8. As a Miva 用户，我想在 `unsafe` 函数中使用 `guard`，这样只有明确标记 unsafe 的代码才能操作 mutex。
9. As a Miva 用户，我希望 Guard 的 drop 行为与 Miva 现有的 drop 系统一致（编译器在作用域出口插入 drop 调用），这样行为可预测且符合语言整体设计。
10. As a Miva 测试者，我希望有一个示例程序展示 Guard 的基本用法，这样用户可以直接参照学习。
11. As a Miva 用户，我希望 Guard 支持在 `if`/`choose`/`while` 等复合语句中使用，这样嵌套作用域也能正确析构。
12. As a Miva 用户，我希望 `drop(g)` 显式调用也能解锁，这样需要在作用域前提前释放锁时仍可行。

## Implementation Decisions

### D1. Guard 结构

`MutexGuard` 是一个简单 struct，只持有一个 `ptrany` 句柄（指向底层 `std::mutex`）。不持有 `Mutex` 所有权，只是借用其句柄。

### D2. 注册 op_drop

通过 `impl MutexGuard { op_drop drop_guard }` 注册析构函数，函数体调用 `mutex_unlock(handle)`。这是 Miva 已有的标准模式（参考 `examples/drop-system/src/main.miva`）。

### D3. lock 返回 Guard

`guard(ref m: Mutex): MutexGuard` 内部先调 `mutex_lock`，再构造并返回 `MutexGuard`。因为参数是 `ref`，`m` 不被消耗，可在同一 mutex 上多次调用 `guard`。

### D4. 不引入新 builtin

`guard` 是纯 Miva 层函数（调用已有 `mutex_lock` builtin），不需要在 `BUILTIN_FUNCTIONS` 或三个后端的 builtin 表中新增条目。

### D5. 向后兼容

原有 `create`/`lock`/`unlock`/`free` 全部保留，`guard` 是新增函数，不是替换。

### D6. Drop 系统复用

不修改任何 drop 相关代码gen。`MutexGuard` 注册 `op_drop` 后，编译器 `droppable.rs` 自动将其纳入 droppable 集合，`drop_desugar.rs` 自动插入析构调用。这与 `File`、`Handle` 等用户自定义 droppable 类型的行为完全一致。

### D7. ptrany 自动使 Guard move-only

`is_copy_type` 对含 `ptrany` 字段的 struct 返回 `false`，因此 `MutexGuard` 自动成为 move-only，无需显式声明。任何尝试拷贝 Guard 的行为会被 move-check 拒绝（E0001）。

## Testing Decisions

### T1. 示例程序测试

在 `examples/mutex-guard/` 新增示例，展示：
- `guard` 的基本用法（lock → 临界区 → 自动 unlock）
- 多条退出路径（`if`/`return`）中 Guard 仍自动解锁
- 显式 `drop(g)` 提前解锁
- 同一 Mutex 上多次 `guard` 调用

### T2. 编译测试

- 验证 Guard 析构时确实调用了 `mutex_unlock`（通过 C++ backend 生成的代码检查）
- 验证 Guard 不可拷贝（尝试赋值/传值应报 E0001）
- 验证原有 `lock`/`unlock` 路径仍正常工作

### T3. 回归测试

运行现有 `examples/mutex/` 示例，确保未受影响。

### Prior Art

- `examples/drop-system/src/main.miva` 展示了自定义 drop 的完整模式，可参照测试 Guard 的析构行为。
- `.scratch/std-mutex/issues/` 中的 `05-integration-test.md` 覆盖了现有 Mutex API 的测试基线。

## Out of Scope

- **Reentrant guard**：C++ `std::mutex` 本身不可重入，双重 lock 同一线程会死锁。本 spec 不引入 reentrant mutex 或 `std::recursive_mutex` 支持。
- **Try-lock / timed-lock**：不实现非阻塞或超时锁。
- **Read-write lock**：不实现 `RwLock` 或 read/write 分离锁。
- **Guard 借用检查**：不引入 borrow checker 级别的"同一时刻只有一个活跃 Guard"静态保证（那是 v2 的优化空间）。
- **mvm backend 特化**：Guard 通过纯 Miva 代码实现，不依赖任何 backend 特定逻辑。

## Further Notes

- 本功能完全在 stdlib 层实现，编译器零改动。这符合 ADR-0001 的精神：特性在 frontend 实现，后端零感知。
- `Atomic[T]`（`std/atomic`）内部已经使用了 `Mutex`。未来可以考虑让 `Atomic` 内部也使用 `MutexGuard` 简化其 `load`/`store` 等操作的临界区管理，但这不在本 spec 范围内——那是独立的清理工作。
- 命名选择 `guard` 而非 `locked` 或 `acquire`，与 Rust `std::sync::Mutex::lock()` 返回 `MutexGuard` 的惯例对齐，也符合 C++ `std::lock_guard` 的语义。
