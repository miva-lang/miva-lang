---
status: accepted
---

# Drop via frontend desugaring with static-only ownership analysis

Miva 需要析构机制（`op_drop`），而三个后端（cxx/llvm/mvm）成熟度不一，历史上后端各自实现的特性会语义漂移（运算符重载在 llvm 是 stub、在 mvm 不看类型）。我们决定：drop 调用在前端 move-check 之后统一脱糖为普通函数调用插入 AST 作用域出口，后端零改动；并且完全静态——不引入运行时 drop flag，代价是编译期直接禁止带 drop 类型的"分支不一致 move"（配套内建 `drop(x)` 作为逃生通道）。

## Considered Options

- 各后端自行实现（cxx 映射 C++ 析构函数、mvm 加 opcode）——被否：三倍工作量 + 已被证实的语义漂移风险。
- Rust 式运行时 drop flag——被否（v1）：需要合成隐藏布尔变量或改动值表示，成本高；静态禁止 + `drop(x)` 覆盖绝大多数场景，flag 留作后续演进。

## Consequences

- droppable 类型（含传染的 drop glue）自动 move-only；v1 禁止 droppable 作泛型实参（`future[T]` 随之被禁）。
- 任何后端差异都不可能影响"何时析构"——析构点在 AST 层已完全确定。
