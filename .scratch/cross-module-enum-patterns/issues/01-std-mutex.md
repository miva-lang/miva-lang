# 01 — std/mutex: 线程锁模块

**What to build:** `std/mutex` 标准库模块，为 Miva 语言提供线程锁功能。Miva 目前没有原生线程锁，该模块通过 C++ 运行时内置函数（`mvp_builtin.h`）包装 `std::mutex`，以 `ptrany` 句柄形式暴露给 Miva 代码使用。

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

## 需求描述

用户需要在 Miva 代码中使用互斥锁保护共享资源，防止并发访问导致的数据竞争。由于 Miva 的 async 机制基于 OS 线程（`async` 函数通过 `std::thread` 派生独立线程），多个 async 任务可能同时访问同一变量，需要互斥锁来保证线程安全。

## 实现方案

### 1. C++ 运行时层（`stdlib/mvp_builtin.h`）

新增以下内置函数：

- `mvp_mutex_new()` — 创建新的 `std::mutex`，返回 `mvp_builtin_ptrany` 句柄
- `mvp_mutex_lock(handle)` — 获取锁，阻塞直到可用
- `mvp_mutex_unlock(handle)` — 释放锁
- `mvp_mutex_free(handle)` — 销毁 mutex 并释放内存

所有函数需包含 null handle 检查，失败时调用 `mvp_panic`。

### 2. Miva 标准库层（`stdlib/std-0.1.3/src/mutex.miva`）

参照 `std/json` 的模式，提供友好的 Miva 接口：

```
module std.mutex;

Mutex = struct {
    handle: ptrany,
}

new = (): Mutex => struct Mutex { handle = mvp_mutex_new() }
unsafe lock = (ref m: Mutex) => mvp_mutex_lock(m.handle)
unsafe unlock = (ref m: Mutex) => mvp_mutex_unlock(m.handle)
unsafe free = (ref m: Mutex) => mvp_mutex_free(m.handle)

export Mutex;
export new;
export lock;
export unlock;
export free;
```

- `Mutex` 结构体仅包含一个 `ptrany` 句柄字段
- 所有操作标记为 `unsafe`（因为涉及裸指针操作）
- 遵循 RAII 模式：`new` 创建，`free` 销毁

### 3. 使用示例

```python
import "std/mutex";

async def worker(mutex):
    mutex.lock();
    try:
        # 临界区代码
        print("doing critical work");
    finally:
        mutex.unlock();

def main():
    mut mtx = std.mutex.new();
    async_worker_1(mtx);
    async_worker_2(mtx);
```

## 用户故事

1. 作为 Miva 开发者，我希望有一个互斥锁类型，这样我可以在多任务间同步共享资源
2. 作为 Miva 开发者，我希望锁操作是 `unsafe` 的，这样我能意识到潜在风险
3. 作为 Miva 开发者，我希望有 `new()` 和 `free()` 方法，这样我能管理锁的生命周期
4. 作为 Miva 开发者，我希望有 `lock()` 和 `unlock()` 方法，这样我能保护临界区
5. 作为 Miva 开发者，我希望 `Mutex` 是非泛型的简单结构体，这样 API 简洁易用
6. 作为 Miva 开发者，我希望遵循 `std/json` 的设计模式，这样 API 风格一致
7. 作为 Miva 开发者，我希望在 async 函数中能安全使用 Mutex，这样我能保护并发数据
8. 作为 Miva 开发者，我希望忘记锁时调用 `free()` 能正确清理资源，这样不会内存泄漏

## 测试方案

- **单元测试**：在 `miva/src/macro_expand.rs` 中测试 `std/mutex` 导入是否正确注入
- **集成测试**：编写一个 Miva 脚本，创建 Mutex，在 async 函数中使用 lock/unlock 保护共享变量，验证输出顺序正确
- **边界测试**：测试重复 lock 行为（std::mutex 不可重入，应 panic 或死锁）、忘记 unlock 的后果、free 后再次操作的错误处理

## 技术决策

- 使用 C++ `std::mutex` 而非 `QMutex`，因为运行时已依赖 C++ 标准库
- 句柄类型为 `ptrany`（`void*`），与 `std/json`、`std/vec` 保持一致
- 不实现读写锁（RwLock），留待后续版本
- 不提供自动上下文管理器（如 Python 的 `with`），保持 API 极简
- 所有方法标记 `unsafe`，因为直接操作裸指针

## 影响范围

- **修改文件**：`stdlib/mvp_builtin.h`（新增 4 个 C++ 函数）
- **新建文件**：`stdlib/std-0.1.3/src/mutex.miva`（Miva 标准库模块）
- **无需修改**：编译器前端、类型检查器、代码生成器（现有 `ptrany` 类型已支持）

## 后续扩展

- `std/rwlock` — 读写锁（`std::shared_mutex`），支持多读单写
- `std/condition_variable` — 条件变量，配合 Mutex 使用
- `std/thread` — 原生线程管理（非 async 模型）
