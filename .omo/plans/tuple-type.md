# tuple-type - Work Plan

## TL;DR (For humans)

**What you'll get:** 元组类型完整支持 — 语法 `(a, b)`、字面量 `(1, true)`、字段访问 `t.0`、let 解构 `let (a, b) = t`，覆盖三个后端（cxx/llvm/mvm）的类型系统、语义分析、代码生成和 drop 管理。

**Why this approach:** 在 `Typ` 枚举中添加独立的 `TTuple` 变体，而非通过合成结构体实现——因为 Miva 的元组类型需要在类型位置出现（函数签名、let 注解），且 `types_equal` 和 droppable 传播需要原生支持。

**What it will NOT do:** 不支持单元素元组（`(int)` 报错）、不支持 `choose` 中的元组解构、不支持元组作为泛型参数（沿用现有 E0036 约束）、不支持函数参数元组解构。

**Effort:** Large
**Risk:** Medium - 涉及前端到三个后端的完整流水线修改，需要后端一致性验证
**Decisions to sanity-check:** 空元组 `()` 作为 unit 类型、copyable 递归原则（所有元素 copyable 则元组 copyable）、元组 drop 按左到右顺序

Your next move: approve and run `$start-work tuple-type`. Full execution detail follows below.

---

> TL;DR (machine): Large, Medium risk — tuple type across frontend/parser/semantic/typecheck/drop/cxx/llvm/mvm

## Scope
### Must have
- `Typ::TTuple { elems: Vec<Typ> }` 变体
- `Expr::ETupleLit { loc, values: Vec<Expr> }` 变体
- 解析器支持类型位置 `(t1, t2)` 和表达式位置 `(e1, e2)`
- 解析器支持 `let (a, b) = expr` 解构绑定（嵌套支持）
- 字段访问 `t.0`, `t.1` 类型推断和代码生成
- 类型相等比较（所有元素类型一致且有序）
- Droppable 传播（任一元素 droppable 则元组 droppable）
- 三个后端均支持：cxx_ir、llvm、mvm
- Drop glue 按元素顺序左到右 drop
- 后端一致性测试（backend_parity）

### Must NOT have (guardrails, anti-slop, scope boundaries)
- 单元素元组 `(int)` — 报错 E0016
- `choose` 中的元组解构模式
- 函数参数解构 `fn foo((a, b): (int, int))`
- 元组作为泛型参数（沿用 E0036 droppable 检查）
- 元组 clone（无 clone trait 系统）
- 空元组 `()` 作为独立 unit 类型 — 保持现有 `TNull` 语义，`()` 字面量返回 `TNull`
- 元组比较运算符 `<=`, `<`, `>=`, `>` — 仅支持 `==` 和 `!=`

## Verification strategy
- Test decision: tests-after + 后端一致性回归测试
- 框架: cargo test --workspace
- 证据: .omo/evidence/tuple-type/ 目录下各 task 的测试输出
- 关键验证: `miva/tests/backend_parity.rs` 中新增元组测试用例

## Execution strategy
### Parallel execution waves
- Wave 1: AST + Parser（基础，阻塞所有后续）
- Wave 2: Semantic Analysis + Type Check（并行）
- Wave 3: Drop Desugaring + CXX Backend（并行）
- Wave 4: LLVM Backend + MVM Backend（并行）
- Wave 5: Integration Tests + Backend Parity

### Dependency matrix
| Todo | Depends on | Blocks | Can parallelize with |
| --- | --- | --- | --- |
| 1. AST + Parser | — | 2-7 | — |
| 2. Semantic | 1 | 3 | 3 |
| 3. Type Check | 1 | 4-7 | 2 |
| 4. Drop Desugar | 1,2,3 | 5 | — |
| 5. CXX Backend | 1,3,4 | 6,7 | — |
| 6. LLVM Backend | 1,3,4 | 7 | 5 |
| 7. MVM Backend | 1,3,4 | — | 6 |
| 8. Integration | 5,6,7 | — | — |

## Todos
> Implementation + Test = ONE todo. Never separate.

- [ ] 1. AST 前端 + Parser 元组语法
  What to do / Must NOT do: 在 miva-frontend-rs/src/ast.rs 添加 Typ::TTuple 和 Expr::ETupleLit；在 parser.rs 添加元组类型解析（parse_typ 中检测 LParen 后跟类型列表）和元组字面量解析（parse_primary 中检测 LParen 后跟表达式列表）；区分 lambda `(x: int): int =>` 和元组 `(1, 2)` 的歧义；添加空元组 `()` 解析为 EVoid；解析器测试覆盖。
  Parallelization: Wave 1 | Blocked by: — | Blocks: 2-7
  References: miva-frontend-rs/src/ast.rs:22-80 (Typ enum), miva-frontend-rs/src/ast.rs:218-354 (Expr enum), miva-frontend-rs/src/parser.rs:769-874 (parse_typ), miva-frontend-rs/src/parser.rs:1634-1776 (parse_primary), miva-frontend-rs/src/parser.rs:1779-1819 (is_lambda_head)
  Acceptance criteria (agent-executable): `cargo test -p miva-frontend-rs` 全部通过；新增 tuple 相关测试用例（元组类型解析、元组字面量解析、单元素报错、嵌套解构）
  QA scenarios (name the exact tool + invocation): happy: 运行 `cargo test -p miva-frontend-rs -- tuple` 确认新测试通过；failure: 运行 `cargo test -p miva-frontend-rs` 确认无回归
  Commit: Y | feat(frontend): add TTuple type and ETupleLit expression with parser support

- [ ] 2. 语义分析：元组 move 检查和 droppable 传播
  What to do / Must NOT do: 在 miva/src/semantic/mod.rs 中处理 ETupleLit 和 TTuple：ETupleLit 递归检查所有元素表达式；TTuple droppable 检查递归到每个元素；泛型参数 ban 递归到 tuple 元素；SLet 中处理元组解构绑定（递归处理嵌套）。Must NOT: 修改其他语义检查逻辑。
  Parallelization: Wave 2 | Blocked by: 1 | Blocks: 4
  References: miva/src/semantic/mod.rs:234-426 (check_expr), miva/src/semantic/mod.rs:600-699 (SLet handling), miva/src/semantic/mod.rs:54-66 (BanCtx::typ), miva/src/droppable.rs:8-22 (is_droppable_typ)
  Acceptance criteria (agent-executable): `cargo test -p miva -- semantic::tests` 全部通过
  QA scenarios: happy: 运行 `cargo test -p miva semantic` 确认通过；failure: 运行 `cargo test -p miva` 确认无回归
  Commit: Y | feat(semantic): add tuple move checking and droppable propagation

- [ ] 3. 类型检查：元组类型推断和解构绑定
  What to do / Must NOT do: 在 miva/src/typecheck/infer.rs 中处理 ETupleLit（推断类型为 TTuple{elems: [...]} 并验证所有元素类型一致）、EFieldAccess（当 expr 类型为 TTuple 时返回第 i 个元素类型）、SLet 元组解构（递归解构绑定类型到 env.vars）；在 miva/src/typecheck/mod.rs 的 types_equal 中添加 TTuple 比较；添加 E0014 元组类型不匹配错误；添加 E0016 单元素元组报错。
  Parallelization: Wave 2 | Blocked by: 1 | Blocks: 4,5,6,7
  References: miva/src/typecheck/infer.rs:1144-1188 (EArrayLit type inference), miva/src/typecheck/infer.rs:1060-1143 (EFieldAccess), miva/src/typecheck/infer.rs:1229-1421 (EBlock SLet handling), miva/src/typecheck/mod.rs:30-81 (types_equal)
  Acceptance criteria (agent-executable): `cargo test -p miva -- typecheck::tests` 全部通过
  QA scenarios: happy: 运行 `cargo test -p miva typecheck` 确认通过；failure: 运行 `cargo test -p miva` 确认无回归
  Commit: Y | feat(typecheck): add tuple type inference and destructuring binding

- [ ] 4. Drop 反语法糖：元组 drop glue
  What to do / Must NOT do: 在 miva/src/drop_desugar.rs 中，当变量类型为 TTuple 且至少一个元素 droppable 时，在作用域出口插入 drop 调用序列（左到右顺序）；复用现有结构体 drop glue 逻辑，将 tuple 元素视为"匿名字段"。Must NOT: 修改结构体 drop 逻辑。
  Parallelization: Wave 3 | Blocked by: 1,2,3 | Blocks: 5
  References: miva/src/drop_desugar.rs:8-79 (desugar_drops entry), miva/src/drop_desugar.rs:100-140 (State declare/types/moved), miva/src/droppable.rs:29-70 (compute_droppable)
  Acceptance criteria (agent-executable): `cargo test -p miva -- drop_desugar` 全部通过，新增元组 drop 测试
  QA scenarios: happy: 运行 `cargo test -p miva drop_desugar` 确认通过；failure: 运行 `cargo test -p miva` 确认无回归
  Commit: Y | feat(drop): add tuple drop glue generation left-to-right

- [ ] 5. CXX 后端：元组代码生成
  What to do / Must NOT do: 在 miva/src/codegen/cxx.rs 的 cxx_type 中添加 TTuple 映射到 C++ `std::tuple<...>`；在 miva/src/codegen/cxx_ir/lower.rs 中添加 ETupleLit → IrExpr 转换；在 miva/src/codegen/cxx_ir/emit.rs 中添加 TupleInit 发射为 `std::make_tuple(...)` 和 FieldAccess 发射为 `std::get<N>(...)`, `==`/`!=` 使用 `std::tie` 比较。Must NOT: 添加新 IrExpr 变体（复用现有模式，tuple 作为 struct-like 处理）。
  Parallelization: Wave 3 | Blocked by: 1,3,4 | Blocks: 6,7
  References: miva/src/codegen/cxx.rs:29-80 (cxx_type), miva/src/codegen/cxx_ir/lower.rs:1-185 (lower_expr), miva/src/codegen/cxx_ir/emit.rs:1-50 (emit_expr), miva/src/codegen/cxx_ir/emit.rs:105-117 (emit_field_access), miva/src/codegen/cxx_ir/emit.rs:119-146 (emit_struct_lit)
  Acceptance criteria (agent-executable): `cargo test -p miva -- codegen::cxx_ir::tests` 全部通过
  QA scenarios: happy: 运行 `cargo test -p miva cxx_ir` 确认通过；failure: 运行 `cargo test -p miva` 确认无回归
  Commit: Y | feat(cxx): add tuple code generation using std::tuple

- [ ] 6. LLVM 后端：元组代码生成
  What to do / Must NOT do: 在 miva/src/codegen/llvm/mod.rs 的 collect_struct_types 中添加 TTuple 生成 LLVM tuple 类型 `%{i64, i64, ...}`；在 miva/src/codegen/llvm/expr.rs 中添加 ETupleLit 生成 alloca + store 各元素；在 miva/src/codegen/llvm/expr.rs 中添加 TTuple FieldAccess 生成 gep + load；添加 tuple 比较支持（== 和 !=，递归比较元素）。Must NOT: 修改现有 struct 代码生成。
  Parallelization: Wave 4 | Blocked by: 1,3,4,5 | Blocks: 7
  References: miva/src/codegen/llvm/mod.rs:58-92 (collect_struct_types), miva/src/codegen/llvm/expr.rs:405-420 (EStructLit), miva/src/codegen/llvm/expr.rs:421-499 (EFieldAccess), miva/src/codegen/llvm/expr.rs:55-128 (EBinOp)
  Acceptance criteria (agent-executable): `cargo test -p miva -- codegen::llvm::tests` 全部通过
  QA scenarios: happy: 运行 `cargo test -p miva llvm` 确认通过；failure: 运行 `cargo test -p miva` 确认无回归
  Commit: Y | feat(llvm): add tuple IR generation with alloca stores and gep loads

- [ ] 7. MVM 后端：元组代码生成
  What to do / Must NOT do: 在 miva/src/codegen/mvm.rs 中添加 ETupleLit 编译为 PushStruct（或新的 PushTuple 操作码，复用 StructNew 模式）；添加 TTuple FieldAccess 编译为 StructGet 模式（字段索引即元素索引）；在 MvmCodegen 的 struct_field_indices 中为元组类型建立虚拟字段映射（索引 0, 1, 2...）。Must NOT: 添加新 opcode（复用 StructNew/StructGet）。
  Parallelization: Wave 4 | Blocked by: 1,3,4,5,6 | Blocks: —
  References: miva/src/codegen/mvm.rs:677-701 (EStructLit), miva/src/codegen/mvm.rs:702-739 (EFieldAccess), miva/src/codegen/mvm.rs:273-318 (collect_struct_info)
  Acceptance criteria (agent-executable): `cargo test -p miva -- codegen::mvm` 全部通过
  QA scenarios: happy: 运行 `cargo test -p miva mvm` 确认通过；failure: 运行 `cargo test -p miva` 确认无回归
  Commit: Y | feat(mvm): add tuple bytecode generation reusing struct opcodes

- [ ] 8. 集成测试 + 后端一致性验证
  What to do / Must NOT do: 在 miva/tests/backend_parity.rs 中添加元组测试用例（基本元组创建/访问、元组作为函数返回值、元组解构、元组比较、嵌套元组、含 droppable 元素的元组 drop）；运行三个后端编译同一源文件并比较输出；验证 drop 行为正确。Must NOT: 添加超出上述范围的测试。
  Parallelization: Wave 5 | Blocked by: 5,6,7 | Blocks: —
  References: miva/tests/backend_parity.rs (existing parity tests), miva/src/codegen/mod.rs (build_ir_with_backend entry)
  Acceptance criteria (agent-executable): `cargo test --workspace` 全部通过；`miva run -b cxx`/`miva run -b llvm`/`miva run -b mvm` 对元组测试文件均输出相同结果
  QA scenarios: happy: 运行 `cargo test --workspace` 确认全部通过；failure: 检查具体失败的测试用例
  Commit: Y | test(integration): add tuple tests and backend parity verification

## Final verification wave
> Runs in parallel after ALL todos. ALL must APPROVE. Surface results and wait for the user's explicit okay before declaring complete.
- [ ] F1. Plan compliance audit
- [ ] F2. Code quality review
- [ ] F3. Real manual QA
- [ ] F4. Scope fidelity

## Commit strategy
每个 todo 独立 commit，遵循 Conventional Commits 格式。Commit message 使用英文。

## Success criteria
1. `cargo test --workspace` 全部通过
2. 后端一致性：同一元组程序在 cxx/llvm/mvm 三个后端输出相同结果
3. 新增测试覆盖：元组基本操作、解构、比较、drop、嵌套
4. 无回归：现有测试全部通过
