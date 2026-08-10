# Miva 工具链架构优化计划

> 状态：**已执行**（详见 [architecture-optimization-status.md](./architecture-optimization-status.md)）
> 原日期：2026-07-27 · 完成日期：2026-07-28
> 范围：miva / miva-frontend-rs / miva-vm / miva-verify 四个 crate 及构建脚本
> 原则：分阶段小步推进，每阶段独立可验证，三后端 examples 输出对拍作为回归兜底

---

## 一、现状诊断

全仓库 53 个 .rs 文件、约 36,600 行。整体是一条多进程流水线：

```
miva-frontend（独立进程, JSON AST over stdout）
    ↓
miva（语义检查 → 类型检查 → drop 脱糖 → 三后端 codegen）
    ↓
g++ / llc / mvm（miva-vm 解释器 + Cranelift JIT）
```

### P0 架构级问题

| # | 问题 | 证据 |
|---|------|------|
| 1 | **无 Cargo workspace**：4 个 crate 各自独立 Cargo.lock 和 target，依赖版本可漂移，miva-vm 作为 path 依赖被重复编译，根 build.sh 就是 workspace 缺失的补丁 | 根目录无 Cargo.toml |
| 2 | **AST 双份手工同步**：同一 JSON schema 在两个 crate 各维护一份镜像，无共享 crate、无 schema 校验，任何单侧字段改动即静默不兼容 | `miva-frontend-rs/src/ast.rs`（491 行，Serialize）↔ `miva/src/ast.rs`（462 行，Deserialize），唯一实质差异是后者 ELambda 多 `captures` 字段 |
| 3 | **前端集成靠子进程 + 路径猜测**：编译器硬编码约 8 条相对路径（`../../../miva-frontend-rs/target/...`）探测前端二进制，fork 后从 stdout 读 pretty JSON；`find_mvm()` 同样猜路径。目录布局被烧进编译器逻辑 | `miva/src/commands/frontend.rs:11-99` |
| 4 | **每文件重复解析 4~5 次**：一次 build 中，import 扫描、宏收集、签名收集、正式编译各 fork 前端一次；mvm 后端还有第 5 次全量重收集。每次都完整 JSON 序列化/反序列化，AST 从不复用 | `miva/src/commands/build.rs` 各 Phase |
| 5 | **生产代码调试遗留**：每个函数体 codegen 都写 `/tmp/analyze_block_debug.txt`，并发编译互相覆盖，Windows/只读 /tmp 静默失败 | `miva/src/codegen/cxx.rs:404` |
| 6 | **测试资产错位**：cxx.rs 里约 1400 行旧 C++ 发射器已不在生产路径（生产走 cxx_ir.rs），却挂着全仓库最多的 103 个 `#[test]`；真正的生产后端 cxx_ir（2229 行）/ llvm（2614 行）/ mvm（1512 行）分别只有 2 / 2 / 0 个测试 | `cxx.rs:1892` `build_ir` 直接委托 `cxx_ir::build_ir` |

### P1 巨型文件与巨型函数

超过 800 行的文件共 14 个，最大者：

| 文件 | 行数 | 混杂职责 |
|---|---|---|
| miva/src/typecheck.rs | 4931 | 泛型规范化、builtin 签名表、shape 检查、核心推导、seal 检查、lambda 捕获标注（独立 AST pass） |
| miva/src/semantic.rs | 3105 | move 检查、droppable 传染、safe/unsafe、Copy 计算、magical 校验、module 检查 |
| miva/src/codegen/cxx.rs | 2858 | 大半为废弃旧发射器 + /tmp 调试写入 |
| miva/src/codegen/llvm.rs | 2614 | 文本拼接 IR；`body_ends_in_terminator` 等函数靠扫描自己刚生成的 IR 文本修补控制流标签（llvm.rs:718-808） |
| miva-frontend-rs/src/parser.rs | 2391 | 体量过大，错误类型是裸 String |

超长函数（>500 行）：`typecheck::infer_type` ~1520 行、`vm::call_builtin` ~860、`vm::execute_loop` ~835、`llvm::gen_expr` ~709、`build::exec` ~617（整条 build 流水线一个函数）、`mvm::compile_expr` ~556、`semantic::check_expr` ~540。

### P1 重复代码

- cxx.rs ↔ cxx_ir.rs 存在成对 `*_ir` 后缀复制：闭包注册表、枚举注册表（函数体逐行相同）、`is_panic`、`collect_generic_params`、`generate_test/header/with_scope`、`collect_exported_rec`。
- 三后端各自维护 builtin 映射表（cxx.rs:262、llvm.rs:249、mvm 自有编码），且 mvm 的内置函数编号表（如 `("ptr_alloc", 79)`）需与 `miva-vm/src/vm.rs` **人肉同步**，历史上已产生成串 bug（见 docs/mvm-backend-debug.md）。
- `build.rs` 中 LLVM bridge 编译逻辑整段复制两遍（1061-1098 与 1104-1146）；libhost 生成在 mvm/llvm 分支各一份（913-951 与 985-1024）。
- Error / Warning 格式化函数逐行同构（error.rs:40-61 ↔ warning.rs:25-46）。
- 转义处理三处实现（frontend util.rs:7、codegen/mod.rs:100、cxx.rs:72）。

### P2 健壮性

- **错误处理四种风格并存**：miva 用 anyhow + 自定义 Error{code,loc}；frontend 与 vm 全 crate `Result<_, String>`；无 thiserror。
- **miva-vm 裸奔**：解释器热路径 ~50 个 unwrap（vm.rs:662-694 整片 `.as_i64().unwrap()`，还有除零 panic）——畸形字节码直接 panic VM；`jit/opcodes.rs:220-367` 有 16 个 `panic!` 位于 extern "C" JIT 回调中，**panic 跨 FFI 边界展开是 UB**。全 crate 仅 4 个测试。
- 死代码：`build.rs:66` `_write_dep_cache` 未被调用（依赖缓存"只读不写"，逻辑残缺）；ast.rs 上 8 处 `#[allow(dead_code)]`。
- 工具链命令硬编码：`g++` / `llc` 直接 `Command::new`，`test_cmd.rs:73` 另有一套不与 build.rs 共享的 g++ 调用。

### P3 仓库卫生

- `miva/test/build/**`（.o/.sha256/.h）和多数 examples 的 build/ 产物已签入 git。
- 根 README.md 严重过时（仍描述单 C++ 后端），与 CONTEXT.md 不符。
- `.gitignore` 忽略 `tests/`，全仓库没有任何集成测试目录；`miva test` 只支持 cxx 后端；miva-verify 是无人调用的孤岛。
- stdlib 靠目录拷贝做版本化，`str.miva` 特殊注入硬编码在 build.rs。

---

## 二、已确认的方向性决策

1. **前端直接链接为库**：miva 通过 Cargo path 依赖进程内调用 `miva_frontend::parse()`，AST 只保留一份定义，彻底消灭 JSON 管道、路径猜测和重复解析。保留 `miva-frontend` 独立二进制供调试。
2. **旧 cxx 发射器：迁测试后删除**：103 个测试改为针对 cxx_ir 生产路径断言，验证等价后删除旧发射器。
3. **分阶段执行**：每阶段独立提交、独立验证，不做大爆炸式重写。

---

## 三、分阶段实施计划

### 第一阶段：地基（workspace + 前端库化）

**1.1 建立 Cargo workspace**
- 根目录新建 `Cargo.toml`：`[workspace] members = ["miva", "miva-frontend-rs", "miva-vm", "miva-verify"]`，`resolver = "2"`；serde / serde_json / anyhow 提升到 `[workspace.dependencies]`。
- 删除 4 个子 crate 的 Cargo.lock，统一到根 lock；`.gitignore` 增加 `/target`。
- 根 build.sh 简化为 `cargo build --workspace [--release]`；子 crate build.sh 删除。

**1.2 AST 单一化 + 前端库化**
- 以 `miva-frontend-rs/src/ast.rs` 为唯一 AST 定义：补 `Deserialize` derive；`ELambda` 增加 `captures` 字段（`#[serde(default)]`）。合并前逐类型 diff 两份定义，确认 serde 标签一致。
- 删除 `miva/src/ast.rs`，miva 内改为 `use miva_frontend::ast::*`（可经薄 re-export 模块减小 diff 面）。
- `miva/Cargo.toml` 增加 `miva-frontend = { path = "../miva-frontend-rs" }`。
- 重写 `commands/frontend.rs`：`run_frontend()` 改为进程内 `parse()` 直接返回 AST（零 JSON 往返）；删除 `find_frontend()` 全部路径猜测。
- `json_ast.rs` 视引用情况删除或降级为调试工具。

**1.3 解析缓存**
- build 流水线引入进程内 `HashMap<PathBuf, Program>`：import 扫描、宏收集、签名收集、正式编译、mvm 全量收集共用同一份 AST。

**1.4 清理硬编码与调试遗留**
- 删除 cxx.rs:404 的 /tmp 写入及 debug_info 拼装。
- `find_mvm()` 改为 `$MIVA_MVM` 环境变量 → PATH → workspace 根 target，删除跨仓库相对路径猜测。

**验收标准**
- `cargo build --workspace` + `cargo test --workspace` 全绿。
- 改造前对至少 2 个 examples 项目分别以 cxx / llvm / mvm 记录基线输出；改造后输出逐字节一致。
- 二次 build 命中 sha256 缓存。

### 第二阶段：测试资产归位 + 死代码清除

- cxx.rs 的 103 个测试逐批迁移为针对 `cxx_ir::build_ir` 输出的断言（预期部分断言文本需适配 cxx_ir 的输出差异，逐一确认语义等价）。
- 全部迁移并通过后，删除旧发射器 ~1400 行；`*_ir` 成对复制的辅助函数只保留一份（去掉后缀）。
- 删除 `_write_dep_cache` 或补全依赖缓存写入逻辑（二选一，倾向补全）。
- 清理 ast.rs 上 8 处 `#[allow(dead_code)]`，删除确实无人读的变体/字段。

**验收**：`cargo test` 数量不减（103 个测试全部存活在新路径），三后端对拍不变。

### 第三阶段：巨型文件拆分与单点化

- **typecheck.rs** 拆为：`typecheck/mod.rs`（核心推导）、`generics.rs`（规范化/替换）、`builtins.rs`（签名表）、`shape.rs`、`seal.rs`、`lambda_capture.rs`（独立 pass 移出）；`infer_type` 按表达式类别拆分为子函数。
- **build::exec** 拆为：项目发现 / import 闭包 / Phase 0-1 / 后端编译链接 / libhost 生成等函数；消除 LLVM bridge 与 libhost 的两处整段复制。
- **builtin 表单点化**：mvm 内置函数名→编号表移入 miva-vm 作为唯一常量表，`miva/src/codegen/mvm.rs` 与 `miva-vm/src/vm.rs` 共同引用，消灭人肉同步。
- semantic.rs / warning.rs 的内嵌测试移到各自 `tests` 子模块文件，缩减主文件体量。
- Error/Warning 格式化去重；转义处理收敛为一处。

**验收**：纯移动/拆分，`cargo test` 全绿，三后端对拍不变，无单文件超过 ~1500 行（parser/lexer 可豁免）。

### 第四阶段：VM 加固与测试补全

- 解释器热路径 unwrap → 带错误码的 VM trap（`run()` 返回结构化错误而非 String）；除零等算术错误显式处理。
- JIT extern "C" 回调中的 16 个 panic → `std::process::abort()` 或错误标志回传，消除跨 FFI 展开 UB。
- 为 miva-vm（字节码 round-trip、解释器、JIT）、cxx_ir、llvm、mvm 后端补单元测试。
- 新建根级端到端集成测试：.miva 源码 → 三后端 build + run → 输出对拍（可吸收 miva-verify 逻辑为测试 harness）；移除 .gitignore 对 `tests/` 的忽略。

**验收**：畸形字节码返回错误而非 panic；集成测试覆盖 examples 主要场景并纳入 CI/build.sh --test。

### 第五阶段：仓库卫生

- `git rm --cached` 移除已签入的 build 产物（miva/test/build/**、examples/*/build/**），完善 .gitignore。
- 更新根 README.md 与 CONTEXT.md / 三后端现状对齐。
- frontend / vm 的 `Result<_, String>` 收敛为结构化错误类型（thiserror 或手写 enum），与 miva 诊断层打通。
- 决定 miva-verify 去留：吸收进集成测试 harness 后删除，或纳入 workspace 正式维护。

---

## 四、风险与对策

| 风险 | 对策 |
|---|---|
| 两份 AST 有未察觉的细微差异 | 合并前逐类型 diff；`captures` 用 serde default 兜底；保留一轮 JSON round-trip 测试对照前端真实输出 |
| 前端 panic 从子进程崩溃变为进程内 panic | parse 错误路径本就走 `Result`，行为不变；如担忧可在调用点 catch_unwind 过渡 |
| 旧测试迁移到 cxx_ir 时断言文本不匹配 | 逐批迁移，每批人工确认语义等价，不做机械替换 |
| 拆分巨型文件引入行为漂移 | 第三阶段只做纯移动，禁止顺手改逻辑；对拍兜底 |
| workspace 化后 target 布局变化影响脚本 | find_mvm 等同步改为 env/PATH/workspace target 三级查找 |

## 五、依赖关系

```
阶段一（地基）──▶ 阶段二（测试归位/删死代码）──▶ 阶段三（拆分/单点化）
                                                    │
阶段四（VM 加固/测试）◀── 可与阶段三并行 ────────────┘
阶段五（仓库卫生）：随时可做，建议最后收尾
```
