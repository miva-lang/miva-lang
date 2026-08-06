# Handoff: Miva 元组类型（Tuple Type）实现

## 目标
为 Miva 语言添加元组类型完整支持：语法、类型推断、语义分析、drop 管理、三个后端代码生成、后端一致性验证。

## 当前状态：实现已完成 ✅

- **cargo check**: 0 errors, 72 warnings
- **cargo test --workspace**: 524 passed（含 parity_tuple 后端一致性测试）
- **git diff**: 27 files changed, 722 insertions(+), 136 deletions(-)

## 已完成的变更

### AST + Parser（miva-frontend-rs）
- `Typ::TTuple { elems: Vec<Typ> }` — 元组类型
- `Expr::ETupleLit { loc, values: Vec<Expr> }` — 元组字面量 `(1, true, "hello")`
- `Stmt::SLetTuple { loc, patterns: Vec<String>, expr: Box<Expr> }` — 解构绑定 `let (a, b) = t`
- `parse_typ`: 检测 `(` 后跟类型列表 → `Typ::TTuple`，单元素报错
- `parse_primary`: 检测 `(` 后非 lambda → 元组字面量，空 `()` → `EVoid`
- `parse_let_tuple`: 递归解析嵌套解构模式 `((a, b), c)`

### 类型检查（miva/src/typecheck）
- `infer.rs`: ETupleLit 推断为 TTuple{elems: [...]}
- `infer.rs`: EFieldAccess 对 TTuple 返回第 i 个元素类型，越界报 E0019
- `infer.rs`: EBlock SLetTuple 注册每个绑定变量类型
- `mod.rs`: types_equal 已支持 TTuple 递归比较
- `mod.rs`: loc_of 包含 ETupleLit

### 语义分析（miva/src/semantic）
- `check_expr`: ETupleLit 递归检查所有元素 move/droppable
- `check_expr`: SLetTuple 注册每个模式变量
- `BanCtx::typ`: TTuple 递归检查泛型参数 ban
- `BanCtx::expr`: ETupleLit 递归检查元素
- `is_copy_type`: TTuple 递归检查所有元素

### Drop 管理
- `droppable.rs`: is_droppable_typ — TTuple 任一元素 droppable 则整体 droppable
- `droppable.rs`: droppable_typ_name — 格式 `(T1, T2)`
- `drop_desugar.rs`: ETupleLit 递归推断 droppable 类型
- `drop_desugar.rs`: TTuple emit_glue — 左到右 drop 每个 droppable 元素
- `drop_desugar.rs`: SLetTuple — 声明每个模式变量为 droppable

### 三个后端
- **CXX**: `cxx_type` → `std::tuple<...>`; lower → TupleInit; emit → `std::make_tuple(...)`/`std::get<N>(...)`
- **LLVM**: ETupleLit → alloca + store 各元素; EFieldAccess → gep + load（tuple 无 tag，不 +1）
- **MVM**: ETupleLit → 各元素 push + StructNew; EFieldAccess → StructGet（tuple 用 StructGet 而非 EnumGet）

### 跨模块 match arms（所有文件已添加 ETupleLit/TTuple/SLetTuple 处理）
- codegen/cxx_ir/{mod,lower,emit,optimize}.rs
- codegen/llvm/{expr,defs,analyze}.rs
- codegen/mvm.rs
- macro_expand.rs
- typecheck/{infer,mod,seal,lambda_capture,shape}.rs
- semantic/mod.rs
- drop_desugar.rs
- warning/mod.rs
- json_ast.rs（测试）

## 测试与验证
- 后端一致性测试：`parity_tuple` 通过（cxx/llvm/mvm 三后端输出相同）
- 示例程序：`examples/tuple/src/main.miva`（基本创建/访问/解构/比较/嵌套）
- 全量测试：524 passed，无回归

## 超出范围的项（spec 中已明确排除）
- 单元素元组 `(int)` — 报错
- choose 中的元组解构
- 函数参数解构 `fn foo((a, b))`
- 元组 clone
- 元组 `<`/`>` 等比较运算符

## 后续可做的事
1. 扩展单元测试（单元素报错、类型不匹配、drop 行为）
2. README 补充元组语法说明
3. LLVM 后端零堆分配优化

## 关键文件路径
- Spec: `docs/specs/tuple-type-spec.md`
- Plan: `.omo/plans/tuple-type.md`
- GitHub Issue: #11（含 #12-#16 子任务）
- 示例: `examples/tuple/`
