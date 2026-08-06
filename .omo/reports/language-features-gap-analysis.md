# Miva 语言特性分析报告

## 项目概述

Miva 是一门受 Rust 启发的编程语言，具有所有权语义和确定性析构。项目包含：
- **前端**: `miva-frontend-rs` (词法/语法分析、语义分析、类型检查)
- **后端**: 三种可互换后端 (cxx、llvm、mvm)
- **标准库**: `stdlib/std-0.1.3/`
- **示例**: 21 个示例项目

---

## 一、已实现特性

### 1. 基础类型系统
- 数值类型: `int`, `float32`, `float64`, `char`
- 布尔: `bool`
- 字符串: `string`
- 引用类型: `ptr<T>`, `box<T>`, `ptrany`
- 集合类型: `array<T>`
- 空值: `null`, `unit`
- 函数类型: `fn(T1, T2): R`
- 泛型参数: `genericParam`
- Shape 类型 (结构子类型)

### 2. 所有权与内存管理
- 值默认 move-only (移动语义)
- 确定性析构: `op_drop` + `drop(x)` 内置函数
- 引用参数: `ref` (不可变引用) / `own` (所有权转移)
- 堆分配: `box<T>`
- 指针操作: `addr`, `deref`, `ptr_set`

### 3. 数据类型
- **Struct**: 具名字段的结构体
- **Enum**: 带载荷的枚举 (支持泛型)
- **Shape**: 编译时结构子类型 (类似 Rust trait)
- **Impl 块**: 为类型实现运算符/方法

### 4. 控制流
- `if/elif/else` 表达式
- `choose`/`when`/`otherwise` 模式匹配
- `while`/`loop`/`for` 循环
- `return` 语句

### 5. 函数与高阶特性
- 匿名函数/lambda
- 闭包 (捕获环境变量)
- 函数柯里化支持
- 异步函数: `async fn` + `.await()`
- `future<T>` 类型

### 6. 泛型系统
- 泛型函数: `f[T] = (...) => ...`
- 泛型结构体/枚举: `Vec[T]`, `Result[T, E]`
- 泛型参数约束: `T: Shape1 + Shape2`
- 类型推导 (部分)

### 7. 错误处理
- `Result[T, E]` 枚举 (std.result)
- `Option[T]` 枚举 (std.option)
- `panic(msg)` 运行时中止
- `unwrap`, `expect`, `unwrap_or` 等组合子

### 8. 模块系统
- `module name;` 声明
- `import "path";` / `import "path" as alias;`
- `export symbol;` 导出
- 多模块项目支持

### 9. 运算符重载
- `op_add`, `op_sub`, `op_mul`, `op_div`
- `op_eq`, `op_neq`
- `op_drop` (析构)

### 10. 安全性分级
- `safe` (默认)
- `unsafe` (需显式声明)
- `trusted` (跳过安全检查)

### 11. 测试系统
- `test fn_name = () => { ... }`
- `miva test` 命令运行测试

### 12. 宏系统
- `macro name(params) => body`
- `macro_var` 变量替换

### 13. C 互操作
- `unsafe fn` 调用外部函数
- `cFunc` 嵌入 C 代码
- `cIntro` / `magical` 代码注入

---

## 二、缺失的现代语言特性

### 🔴 高优先级 (影响核心功能)

#### 1. 模式匹配增强

**缺失特性:**
- 元组/结构体模式匹配
- 嵌套模式匹配
- 守卫表达式 (已有 `guard` 字段，但未充分实现)
- 正则表达式匹配
- 类型模式 (`match x as int => ...`)

**现状:** 目前仅支持枚举变体匹配，且只能匹配顶层变体。

**参考实现:**
```rust
// 当前支持的
choose (s) {
    when (Shape.Circle(r)) { ... }
}

// 缺失的 - 元组/结构体模式
match point {
    when (Point { x, y }) { ... }
    when (Point(_, 0)) { ... }  // 忽略某个字段
}

// 缺失的 - 嵌套匹配
match result {
    when (Ok(value)) { ... }
    when (Err("not found")) { ... }  // 匹配错误值
}
```

---

#### 2. Trait/接口系统

**缺失特性:**
- 抽象方法定义
- 多态实现
- 派生属性 (类似 Rust `#[derive(Clone, Debug)]`)
- 特征对象 (trait objects / `dyn Shape`)
- 关联类型 (associated types)

**现状:** 仅有 `Shape` 结构子类型，无抽象方法系统。

**参考实现:**
```rust
// 缺失的 - Trait 定义
Shape Drawable {
    fn draw(self: &Self) -> unit;
    fn description(self: &Self) -> string;
}

// 缺失的 - 实现 Trait
impl Circle: Drawable {
    fn draw(self: &Self) -> unit { ... }
    fn description(self: &Self) -> string { "circle" }
}

// 缺失的 - 类型擦除
let shapes: array<dyn Drawable> = [circle, rect];
for shape in shapes { shape.draw(); }
```

---

#### 3. 错误处理糖语法

**缺失特性:**
- `?` 运算符 (传播运算符)
- `??` 运算符 (提供默认值)
- `let-else` 绑定
- 错误类型推导

**现状:** 必须手动调用 `unwrap()`, `expect()` 等函数。

**参考实现:**
```rust
// 缺失的 - 错误传播
let result = try_read_file("path")?;  // 自动传播错误

// 缺失的 - 默认值
let value = parse_int(input) ?? 0;

// 缺失的 - let-else
let Ok(value) = parse_int(input) else {
    return Err("invalid integer");
};
```

---

#### 4. 迭代器协议

**缺失特性:**
- `Iterator` trait
- `IntoIterator` trait
- `for-in` 范围迭代 (已有 `for i in range`，但无集合迭代)
- 迭代器适配器 (`.map()`, `.filter()`, `.collect()` 等)
- 双向迭代器

**现状:** 仅支持数值范围的 `for` 循环。

**参考实现:**
```rust
// 缺失的 - 集合迭代
let vec = vec::new[int]();
vec::push(vec, 1);
vec::push(vec, 2);
vec::push(vec, 3);

for item in vec {  // 需要 Iterator trait
    printlns!(item);
}

// 缺失的 - 迭代器链
let sum = vec::map(vec, (x: int): int => x * 2)
           .filter(|x| x > 2)
           .reduce(|a, b| a + b);
```

---

#### 5. 闭包捕获改进

**缺失特性:**
- 按值捕获 vs 按引用捕获的显式控制
- `move` 闭包关键字
- 循环引用检测
- 闭包类型推导

**现状:** 自动捕获，无用户控制。

**参考实现:**
```rust
// 缺失的 - move 闭包
let owned = String::from("hello");
let closure = move (): string => owned;  // 转移所有权

// 缺失的 - 显式捕获声明
let closure = [ref x, owned y] (): int => x + y;
```

---

### 🟡 中优先级 (显著提升表达力)

#### 6. 属性/注解系统

**缺失特性:**
- 派生属性 (`#[derive(Clone, Debug)]`)
- 条件编译 (`#[cfg(target_os = "linux")]`)
- 编译器指令 (`#[inline]`, `#[panic_handler]`)
- 自定义属性

**参考实现:**
```rust
// 缺失的 - 派生属性
#[derive(Clone, Debug)]
struct Point { x: int, y: int }

// 缺失的 - 条件编译
#[cfg(target_os = "linux")]
fn platform_init() -> unit { ... }

// 缺失的 - 函数属性
#[inline(always)]
fn fast_add(a: int, b: int): int => a + b;
```

---

#### 7. 常量与编译期计算

**缺失特性:**
- `const` 声明
- `static` 全局常量
- 编译期求值 (`const fn`)
- 常量泛型参数

**现状:** 无常量系统。

**参考实现:**
```rust
// 缺失的 - 常量
const PI: float64 = 3.14159265359;
const MAX_SIZE: int = 1024;

// 缺失的 - 编译期函数
const fn factorial(n: int): int {
    if n <= 1 { return 1; }
    return n * factorial(n - 1);
}

// 缺失的 - 使用常量作为泛型参数
const N: int = 10;
let arr: array[int, N] = [0; N];  // 类似 Rust const generics
```

---

#### 8. 生命周期标注

**缺失特性:**
- 引用生命周期标注
- `'static` 生命周期
- 生命周期省略规则
- 生命周期约束

**现状:** 无显式生命周期系统。

**参考实现:**
```rust
// 缺失的 - 生命周期标注
fn longest<'a>(x: &'a str, y: &'a str): &'a str {
    if x.len() > y.len() { x } else { y }
}

// 缺失的 - 结构体生命周期
struct Ref<'a> {
    data: &'a str,
}
```

---

#### 9. 异常处理

**缺失特性:**
- `try/catch/finally` 语法
- 自定义异常类型
- 异常链

**现状:** 仅支持 `panic`，无恢复机制。

**参考实现:**
```rust
// 缺失的 - 异常处理
try {
    let value = parse_int(input);
} catch (e: ParseError) {
    printlns!("Parse failed: ", e.message);
} finally {
    cleanup();
}

// 缺失的 - 自定义异常
struct IOError { code: int, message: string }
```

---

#### 10. 并发原语扩展

**缺失特性:**
- `spawn`/`thread` 关键字
- `Send`/`Sync` trait
- 原子操作的类型安全包装
- 条件变量
- Channel 类型 (已有 `std::sync::mpsc` 概念但未完整)

**现状:** 有 `future` 和基础 `mutex`/`atomic`，但无线程模型。

**参考实现:**
```rust
// 缺失的 - 线程
let handle = spawn () => {
    // 在独立线程中运行
    expensive_computation();
};
handle.join();

// 缺失的 - Channel
let (tx, rx) = channel[int]();
tx.send(42);
let value = rx.recv();
```

---

### 🟢 低优先级 (锦上添花)

#### 11. 字符串插值增强

**现状:** 支持 `printlns!("...", expr)` 形式。

**缺失特性:**
- 原生字符串字面量 (`r"raw string"`)
- 格式化字符串 (`format!("{} {}", a, b)`)
- 多行字符串 (`"""multiline"""`)

---

#### 12. 数字字面量增强

**缺失特性:**
- 二进制字面量 (`0b1010`)
- 八进制字面量 (`0o77`)
- 十六进制字面量 (`0xFF`)
- 数字分隔符 (`1_000_000`)
- 复数类型 (`complex`)

---

#### 13. 位操作增强

**缺失特性:**
- 移位运算符 (`<<`, `>>`)
- 位与/位或/位异或 (`&`, `|`, `^`)
- 按位取反 (`~`)
- 旋转操作 (`rotate_left`, `rotate_right`)

**现状:** 仅有算术和逻辑运算符。

---

#### 14. 元组类型

**缺失特性:**
- 元组字面量 (`(a, b, c)`)
- 元组解构 (`let (x, y) = pair;`)
- 元组字段访问 (`pair.0`, `pair.1`)

**现状:** 无原生元组类型。

---

#### 15. 枚举增强

**缺失特性:**
- 枚举关联方法
- 枚举到整数转换
- 继承/子类型枚举
- 递归枚举

---

#### 16. 作用域控制

**缺失特性:**
- `loop` 标签 (跳出嵌套循环)
- `continue`/`break` 带标签
- 块表达式返回值 (已有 `EBlock` 的 `result` 字段)

---

#### 17. 调试与反射

**缺失特性:**
- `debug!`/`trace!` 宏
- 运行时类型信息 (RTTI)
- `typeid` 查询
- 序列化的 derive macro

---

#### 18. 包管理与依赖

**缺失特性:**
- 语义化版本约束
- 依赖锁定文件 (已有 `miva.toml` 概念)
- Workspace 支持
- 测试/发布配置

---

## 三、特性对比矩阵

| 特性类别 | 已实现 | 缺失程度 | 优先级 |
|---------|-------|---------|-------|
| 基础类型 | ✅ 完整 | - | - |
| 所有权/内存 | ✅ 基础 | 生命周期缺失 | 高 |
| 模式匹配 | ⚠️ 基础 | 严重不足 | 🔴 高 |
| 泛型 | ✅ 基础 | 约束系统不完善 | 中 |
| Trait/接口 | ❌ 无 | 完全缺失 | 🔴 高 |
| 错误处理 | ⚠️ 基础 | 缺少 `?` 运算符 | 🔴 高 |
| 迭代器 | ❌ 无 | 完全缺失 | 🔴 高 |
| 并发 | ⚠️ 基础 | 缺少线程模型 | 🟡 中 |
| 宏系统 | ⚠️ 基础 | 功能有限 | 🟢 低 |
| 字符串 | ⚠️ 基础 | 缺少格式化 | 🟢 低 |

---

## 四、建议实现路线

### Phase 1: 核心缺失 (影响日常编程)
1. **模式匹配增强** - 元组/结构体解构
2. **Trait 系统** - 抽象方法与多态
3. **错误传播 `?`** - 大幅提升代码简洁性
4. **迭代器协议** - 集合操作的基石

### Phase 2: 表达力提升
5. **常量系统** - 编译期计算
6. **生命周期标注** - 安全的引用管理
7. **属性系统** - 派生宏的基础
8. **异常处理** - 错误恢复能力

### Phase 3: 生态系统完善
9. **并发模型** - 线程/Channel
10. **字符串增强** - 插值/格式化
11. **调试工具** - 宏与反射
12. **包管理完善** - 依赖管理

---

## 五、与 Rust 的差距分析

| Rust 特性 | Miva 状态 | 差距说明 |
|----------|----------|---------|
| Pattern Matching | ⚠️ 基础 | 仅支持枚举变体，无解构 |
| Traits | ❌ 无 | 仅有 Shape 结构子类型 |
| `?` Operator | ❌ 无 | 需手动 unwrap |
| Iterators | ❌ 无 | 无迭代器协议 |
| Lifetimes | ❌ 无 | 引用无生命周期标注 |
| Derive Macros | ❌ 无 | 无派生属性 |
| Const Generics | ❌ 无 | 无编译期常量 |
| Async/Await | ✅ 基础 | 有 future，缺 spawn |
| Thread Safety | ⚠️ 基础 | 有 mutex，缺类型系统保证 |
| Modules/Crates | ⚠️ 基础 | 有 import/export，缺 workspace |

---

## 六、结论

Miva 语言在**所有权语义**和**多后端编译**方面已经实现了良好的基础。然而，在以下核心领域存在明显缺失：

1. **模式匹配** - 仅支持简单的枚举匹配，无法解构复杂类型
2. **Trait/接口系统** - 完全缺失，限制了多态和泛型能力
3. **错误处理** - 缺少 `?` 运算符等糖语法，影响代码可读性
4. **迭代器** - 无迭代器协议，集合操作受限
5. **生命周期** - 无显式生命周期标注，引用安全性依赖编译器推断

这些缺失限制了 Miva 表达复杂领域模型的能力，建议在 Phase 1 优先实现模式匹配、Trait 系统和错误传播运算符。
