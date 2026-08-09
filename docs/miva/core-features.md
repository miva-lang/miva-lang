# Miva 语言核心特性详解

本文档详细介绍 Miva 语言的五个核心特性：**Tuple 类型**、**Shape 约束系统**、**Drop 系统**、**std.atomic 原子操作**、**std.mutex 互斥锁**。这些特性共同构成了 Miva 的类型安全和并发编程基础。

---

## 一、Tuple 类型

Miva 支持同质和异质元组类型作为一等公民值。

### 元组语法

```miva
// 元组类型注解
pair = (x: int, y: int): (int, int) => {
    return (x, y);
}

// 异质元组
mixed = (): (int, bool, string) => {
    return (1, true, "hello");
}

// 嵌套元组
nested = (): (int, (bool, string)) => {
    return (1, (true, "nested"));
}
```

### 元组访问

元组元素通过基于零的位置索引使用字段访问语法：

```miva
sum_pair = (p: (int, int)): int => {
    return p.0 + p.1;
}

main = () => {
    let p = pair(10, 20);
    printlns!(p.0);   // 10
    printlns!(p.1);   // 20

    let m = mixed();
    printlns!(m.0);   // 1
    printlns!(m.1);   // true
    printlns!(m.2);   // "hello"

    let n = nested();
    printlns!(n.1.0); // true（嵌套访问）
    printlns!(n.1.1); // "nested"
}
```

### 元组比较

当所有元素类型可比较时，元组支持相等性比较（`==`, `!=`）：

```miva
let c = compare((1, true), (1, true));  // true
```

### AST 表示

元组类型在 AST 中由 `TTuple` 节点表示：

```rust
// miva-frontend-rs/src/ast.rs
TTuple(Vec<TypeNode>),  // 元组类型，如 (int, bool, string)
```

---

## 二、Shape 约束系统

Miva 支持 **Shape 定义**，它充当结构的编译时契约。Shape 声明一组必需的字段（名称 + 类型）；任何包含至少这些字段的结构都满足 Shape，无论它是否有额外的字段。

### Shape 定义

```miva
PersonShape = shape {
  name: string,
  age: int,
}

HasValue[T] = shape {
  value: T,
}
```

Shape 不是运行时类型——它们在代码生成时被消除，仅用于静态检查。

### Shape 满足规则

一个结构满足 Shape 当它拥有所有必需字段且类型匹配。额外字段是允许的：

```miva
Employee = struct {
  name: string,
  age: int,
}

Customer = struct {
  name: string,
  age: int,
  email: string,  // 额外字段 — 允许
}

main = () => {
  let emp Employee = struct Employee { name = "Alice", age = 30 };
  let cust Customer = struct Customer { name = "Bob", age = 25, email = "bob@example.com" };
}
```

### Shape 约束的类型注解

当变量用 `TShape` 类型声明时（如 `let x PersonShape = ...`），编译器验证赋值给定的结构字面量是否满足 Shape：

```miva
let box1 IntBox = struct IntBox { value = 42 };          // 满足 HasValue[int]
let box2 StringBox = struct StringBox { value = "hello" };  // 满足 HasValue[string]
```

### 泛型 Shape 约束语法

泛型可以使用 `+` 连接多个 Shape 约束：

```miva
// 泛型结构带 Shape 约束
Person = struct[T: PersonShape] {
  data: T,
}

// 泛型函数带 Shape 约束
process_person[T: PersonShape](p: T) => {
  printlns!(p.name);
  printlns!(p.age);
}

// 多约束
Foo[T: ShapeA + ShapeB] = struct { ... }
```

### 错误码

| 错误码 | 描述 |
|--------|------|
| E0028 | Shape 未满足 — 类型缺少必需字段 |
| E0030 | Shape 约束未满足 — 字段类型不匹配 |

### 代码位置

- **定义检查**: `miva/src/typecheck/shape.rs`
- **示例**: `examples/shape-system/src/main.miva`

---

## 三、Drop 系统

Miva 支持自动资源清理的 Drop 系统，类似于 Rust 的 `Drop` trait。结构可以通过 `impl` 块注册析构函数，该函数在值超出作用域时自动运行。

### 基本用法

```miva
File = struct {
  id: int,
}

file_close = (ref self: File) => {
  printlns!("closing file");
  printlns!(self.id);
}

impl File {
  op_drop file_close,
}
```

### 签名和注册规则

- 析构函数必须有精确签名 `(ref self: T)` 且无返回值（E0031）
- 每个结构最多可以注册一个 `op_drop`（E0032）
- 注册的析构函数是**封闭的**：不能直接调用或作为值使用（E0034）。使用 `drop(x)` 代替

### 销毁顺序

销毁是确定性的且基于作用域（类似 Rust）：

- 在作用域退出时，活动可销毁值按**反向声明顺序**销毁
- 对于每个值，其自身的 `op_drop` 首先运行，然后递归销毁其可销毁内容：
  - 结构字段按声明顺序
  - 枚举的活动变体负载
  - 数组元素按索引顺序

可销毁性是传染性的：包含可销毁类型的结构、枚举或数组自身也是可销毁的，并接收递归析构胶水，即使没有自己的 `op_drop`。

### 移动独占语义

可销毁类型是移动独占的：

- 传递或返回它们需要显式 `move`；隐式拷贝被拒绝
- 已移动的值在作用域退出时不会析构
- 在 `if`/`else` 的一个分支中移动可销毁值是错误（E0033）——在两个分支中都移动或都不移动。一个分支中的 `drop(x)` 平衡另一个分支中的 `move`

### 提前销毁：drop(x)

内置函数 `drop(x)` 立即销毁可销毁变量并消耗它：

```miva
early = () => {
  let f File = struct File { id = 1 };
  drop(f);              // file_close 在此运行
  // f 从此处起已移动
}
```

`drop()` 恰好接受一个可销毁变量（E0035）。

### MutexGuard RAII 模式

`std.mutex.MutexGuard` 利用 `op_drop` 实现作用域退出自动解锁：

```miva
let m std.mutex.Mutex = std.mutex.create();
let g std.mutex.MutexGuard = std.mutex.guard(m);
// g 在作用域退出时自动 unlock
// 无需手动调用 unlock，避免忘记解锁的风险
```

### v1 限制

- 可销毁类型不能用作泛型、`future` 或 `box` 参数（E0036）——例如 `Vec[File]`、`future[File]` 和可销毁类型的 `box` 被拒绝。普通数组 (`[File]`) 是允许的
- 析构胶水也在异步函数体中运行；禁止仅适用于跨越 `future[T]` 边界的可销毁值

### 错误码

| 错误码 | 描述 |
|--------|------|
| E0031 | op_drop 函数未定义或签名不是 `(ref self: T)` 且无返回值 |
| E0032 | 同一结构重复注册 op_drop |
| E0033 | 可销毁值仅在 if/else 的一个分支中移动 |
| E0034 | 封闭的 drop 函数被直接调用或用作值 |
| E0035 | drop() 恰好需要一个可销毁变量 |
| E0036 | 可销毁类型用作泛型/future/box 参数（v1 限制） |

### 代码位置

- **Drop 逻辑**: `miva/src/drop_desugar.rs`, `miva/src/droppable.rs`
- **示例**: `examples/drop-system/`

---

## 四、std.atomic — 线程安全原子访问

`std.atomic` 模块提供线程安全的原子操作，适用于并发场景。

### 类型定义

```miva
Atomic[T] = struct {
  buf: ptrany,           // 底层缓冲区
  mutex: std.mutex.Mutex, // 内部互斥锁
  freed: bool            // 是否已释放
}
```

### 构造和销毁

```miva
std.atomic.new[T](): Atomic[T]       // 分配缓冲区；调用者必须调用 free
std.atomic.free[T](ref a: Atomic[T]) // 释放缓冲区和互斥锁（恰好调用一次）
```

### 访问操作（全部 unsafe）

```miva
std.atomic.load[T](ref a: Atomic[T]): T
std.atomic.store[T](ref a: Atomic[T], val: T)
std.atomic.swap[T](ref a: Atomic[T], new_val: T): T
std.atomic.compare_exchange[T](ref a: Atomic[T], expected: T, new_val: T): bool
std.atomic.fetch_add[T](ref a: Atomic[T], val: T): T  // 仅限整数
std.atomic.fetch_sub[T](ref a: Atomic[T], val: T): T  // 仅限整数
```

### 内部辅助

```miva
std.atomic.elem_size[T](): int  // 返回 8（int64_t / double 的大小）
```

### 并发安全保证

所有操作在访问值之前获取内部互斥锁，使其对来自多个 `async` 任务的并发使用是安全的。`compare_exchange` 使用 `==` 进行相等性检查。

### 示例

```miva
import "std/atomic";

main = () => {
  let counter std.atomic.Atomic[int] = std.atomic.new[int]();
  
  std.atomic.store[counter](counter, 0);
  
  async increment = () => {
    mut i := 0;
    while i < 1000 {
      let old std.atomic.Atomic[int] = std.atomic.fetch_add[int](counter, 1);
      i += 1;
    }
  }
  
  // 多个任务并发递增
  let t1 = increment();
  let t2 = increment();
  
  t1.await();
  t2.await();
  
  let result = std.atomic.load[counter](counter);
  printlns!(result);  // 2000
}
```

### 代码位置

- **实现**: `stdlib/std-0.1.4/src/atomic.miva`
- **示例**: `examples/atomic/src/main.miva`

---

## 五、std.mutex — 互斥锁

`std.mutex` 模块提供线程安全的互斥锁，用于同步并发访问。

### 类型定义

```miva
Mutex = struct {
  handle: ptrany  // 底层互斥锁句柄
}

MutexGuard = struct {
  handle: ptrany  // RAII 自动解锁守卫
}
```

### 生命周期操作

```miva
std.mutex.create(): Mutex                         // 堆分配未锁定的互斥锁
std.mutex.lock(ref m: Mutex)                      // 获取锁（阻塞）
std.mutex.unlock(ref m: Mutex)                    // 释放锁
std.mutex.free(ref m: Mutex)                      // 销毁互斥锁（恰好调用一次）
std.mutex.guard(ref m: Mutex): MutexGuard         // 锁定并返回 RAII 守卫
```

### 重要注意事项

- `std.mutex` **不是可重入的**——从同一线程两次锁定同一个互斥锁会死锁
- `MutexGuard` 是移动独占的，因为它包含一个必须恰好释放一次的 `ptrany`
- `MutexGuard` 实现了 `op_drop`：在作用域退出时自动解锁

### 使用示例

#### 手动锁定/解锁

```miva
import "std/mutex";

main = () => {
  let m std.mutex.Mutex = std.mutex.create();
  
  std.mutex.lock[m](m);
  // 临界区...
  std.mutex.unlock[m](m);
  
  std.mutex.free[m](m);
}
```

#### RAII 守卫模式（推荐）

```miva
import "std/mutex";

main = () => {
  let m std.mutex.Mutex = std.mutex.create();
  let g std.mutex.MutexGuard = std.mutex.guard[m](m);
  
  // 临界区...
  // g 在作用域退出时自动 unlock
  // 无需手动调用 unlock，避免忘记解锁的风险
  
  // g 被 drop 时自动释放锁
}
```

### 与 async 结合使用

```miva
import "std/mutex";
import "std/atomic";

main = () => {
  let counter std.atomic.Atomic[int] = std.atomic.new[int]();
  let lock std.mutex.Mutex = std.mutex.create();
  
  async worker(id: int) => {
    std.mutex.lock[lock](lock);
    
    let val = std.atomic.load[counter](counter);
    std.atomic.store[counter](counter, val + 1);
    
    std.mutex.unlock[lock](lock);
  }
  
  let t1 = worker(1);
  let t2 = worker(2);
  
  t1.await();
  t2.await();
  
  std.mutex.free[lock](lock);
  std.atomic.free[counter](counter);
}
```

### 代码位置

- **实现**: `stdlib/std-0.1.4/src/mutex.miva`
- **示例**: `examples/mutex/src/main.miva`, `examples/mutex-guard/src/main.miva`

---

## 六、特性关联与组合使用

这些特性可以组合使用以构建复杂的并发数据结构：

### 组合示例：线程安全的 Vec

```miva
import "std/vec";
import "std/mutex";
import "std/atomic";

SyncVec[T] = struct {
  data: std.vec.Vec[T],
  lock: std.mutex.Mutex,
}

// 创建线程安全的向量
thread_safe_vec[T] = (): SyncVec[T] => {
  let v std.vec.Vec[T] = std.vec.new[T]();
  let m std.mutex.Mutex = std.mutex.create();
  return struct SyncVec[T] { data = v, lock = m };
}

// 线程安全的 push
unsafe push[T](ref sv: SyncVec[T], val: T) => {
  std.mutex.lock[sv.lock](sv.lock);
  std.vec.push[T](ref sv.data, val);
  std.mutex.unlock[sv.lock](sv.lock);
}
```

### 泛型约束 + Drop

```miva
// 泛型结构带 Shape 约束和 Drop
Resource[T: HasValue] = struct {
  value: T,
  handle: ptrany,
}

impl[T: HasValue] Resource[T] {
  op_drop resource_close,
}

resource_close[T: HasValue](ref self: Resource[T]) => {
  // 自动清理资源
}
```

---

## 七、总结

| 特性 | 核心用途 | 关键机制 | 相关错误码 |
|------|----------|----------|------------|
| **Tuple** | 多值返回、临时组合 | 位置索引访问 `.0`, `.1` | E0019（索引越界） |
| **Shape** | 编译时结构契约 | 字段存在性和类型检查 | E0028, E0030 |
| **Drop** | 自动资源清理 | `op_drop` 注册、反向顺序销毁 | E0031-E0036 |
| **std.atomic** | 线程安全原子操作 | 内部互斥锁保护 | - |
| **std.mutex** | 手动/RAII 互斥锁 | `MutexGuard` 自动解锁 | - |

这些特性共同构成了 Miva 的类型安全和并发编程基础，使开发者能够在编译时捕获错误，在运行时保证内存安全和线程安全。
