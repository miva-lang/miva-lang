# Miva 架构优化计划 — 执行状态追踪

> 源文件：`docs/architecture-optimization-plan.md`
> 最后更新：2026-08-09
> 状态：**阶段一~四 基本完成，阶段五部分完成，剩余清理项**

---

## 阶段总览

| 阶段 | 内容 | 状态 |
|------|------|------|
| 第一阶段：地基 | Cargo workspace + 前端库化 + 单一 AST + 解析缓存 | ✅ 已完成 |
| 第二阶段：测试归位 + 死代码清除 | 删除旧 cxx 发射器，迁移测试到 cxx_ir | ✅ 已完成 |
| 第三阶段：巨型文件拆分与单点化 | typecheck/semantic/build/cxx_ir/llvm/vm 拆子模块，builtin 表单点化 | ✅ 已完成 |
| 第四阶段：VM 加固与测试补全 | 解释器 unwrap → trap，JIT panic → abort，集成测试 | ✅ 已完成 |
| 第五阶段：仓库卫生 | README 更新、miva-verify 清理、error 类型收敛 | 🔄 部分完成 |

---

## 已完成工作（按提交时间倒序）

### 阶段一：地基（commit 2e975b4, 2026-07-28）
- [x] 根目录创建 `Cargo.toml` workspace，members = ["miva", "miva-frontend-rs", "miva-vm"]
- [x] `serde` / `serde_json` / `anyhow` 提升到 `[workspace.dependencies]`
- [x] AST 单一化：`miva/src/ast.rs` 改为 `mod ast` re-export，`miva-frontend-rs/src/ast.rs` 为主定义
- [x] 前端库化：`commands/frontend.rs` 从 99 行路径猜测缩减至 65 行进程内调用
- [x] 解析缓存：build 流水线复用 AST，不再每文件 fork 子进程 4~5 次
- [x] 删除子 crate 独立 `Cargo.lock`，统一到根 lock

### 阶段二：测试归位 + 死代码清除（commit 0548d90, 2026-07-28）
- [x] 旧 `cxx.rs` 1400 行废弃发射器已删除（现仅 752 行，仅保留共享 helpers）
- [x] 103 个测试已迁移至 `miva/src/codegen/cxx_ir/tests.rs`（711 行）
- [x] `cxx_ir` 模块拆分：`mod.rs` / `lower.rs` / `emit.rs` / `optimize.rs` / `tests.rs`
- [x] `*_ir` 后缀重复辅助函数已去重，删除旧符号
- [x] 依赖缓存写入逻辑已修复（`.d` 文件缓存，warm build 命中）
- [x] workspace 级测试：`cargo test --workspace` 504 个测试全绿

### 阶段三：巨型文件拆分与单点化（commit 6518b44, 2026-07-28）
- [x] `typecheck.rs` (4931行) → `typecheck/` 目录：
  - `mod.rs` (535行) — 核心推导
  - `infer.rs` (1714行) — infer_type 拆分
  - `generics.rs` (149行)
  - `builtins.rs` (100行)
  - `shape.rs` (135行)
  - `seal.rs` (147行)
  - `lambda_capture.rs` (520行)
- [x] `semantic.rs` (3105行) → `semantic/mod.rs` (1102行) + `semantic/tests/` (2481行)
- [x] `llvm.rs` (2614行) → `llvm/` 目录：
  - `mod.rs` (632行) / `expr.rs` (2033行) / `defs.rs` (864行) / `analyze.rs` (537行) / `tests.rs` (112行)
- [x] `build.rs` (1167行) → `commands/build/` 目录：
  - `mod.rs` (422行) / `cache.rs` (80行) / `compile.rs` (359行) / `host.rs` (81行) / `imports.rs` (103行) / `sigs.rs` (158行)
- [x] `miva-vm/src/vm.rs` → `vm/mod.rs` (1745行) + `vm/call_builtin.rs` (1249行) + `vm/tests.rs` (215行)
- [x] mvm builtin 表单点化：`miva-vm/src/builtins.rs` 单一来源，`miva/src/codegen/mvm.rs` 共同引用
- [x] Error/Warning 格式化去重，转义处理收敛
- [x] 所有三后端对拍验证通过，字节一致

### 阶段四：VM 加固与测试补全（commit 0270450, 2026-07-28）
- [x] 解释器热路径：~50 个 unwrap 替换为结构化 `VmError` trap
- [x] `VmError` 涵盖：invalid bytecode、stack overflow/underflow、division by zero、type error、out of bounds
- [x] JIT extern "C" 回调中 16 个 `panic!` → `std::process::abort()`（消除跨 FFI 展开 UB）
- [x] 畸形字节码返回错误而非 panic
- [x] `miva-vm/src/vm/tests.rs`：loader fuzzing + trap coverage（215 行）
- [x] 端到端三后端对拍集成测试：`miva/tests/backend_parity.rs`（147 行）
- [x] 集成测试纳入 `cargo test --workspace`

### 阶段五：仓库卫生（部分完成）
- [x] `README.md` 已重写，与三后端现状对齐（commit 6706292）
- [x] `miva-verify` 孤岛已删除（commit 6706292）
- [x] `.gitignore` 已更新
- [ ] **未做**：`git rm --cached` 移除已签入的 build 产物（`miva/test/build/`、`examples/*/build/`）
- [ ] **未做**：frontend / vm 的 `Result<_, String>` 收敛为结构化错误类型（thiserror）
  - `miva-vm/src/error.rs` 已有 `VmError`（92 行），但解释器热路径外部接口仍有 `String` 残留
- [ ] **未做**：stdlib 目录拷贝版本化方式待优化（`str.miva` 特殊注入仍硬编码在 build.rs）

---

## 已完成特性计划（不在架构优化范围内，但已落地）

| 计划文件 | 状态 | 说明 |
|----------|------|------|
| `2026-07-18-adt-enum.md` | ✅ 已实现（v0.1.3） | enum/ADT、模式解构、choose守卫 |
| `2026-07-19-generic-enum.md` | ✅ 已实现（v0.1.3） | 泛型 enum |
| `2026-07-20-higher-order-functions.md` | ✅ 已实现（v0.1.3） | TFunc + ELambda + 闭包，三后端支持 |
| `2026-07-21-mvm-jit-tier.md` | ✅ 已实现 | Cranelift JIT + 性能分析，热函数自动编译 |
| `2026-07-24-cross-module-enum-patterns.md` | ✅ 已实现 | 跨模块 enum 模式匹配 |
| `2026-07-26-shape-system-design.md` | ✅ 已实现（v0.1.4） | shape 约束系统，泛型 bound |
| `2026-08-05-mutex-guard-design.md` | ✅ 已实现（v0.1.4） | MutexGuard RAII 包装 |

---

## 当前代码规模（优化后）

| 模块 | 文件 | 行数 |
|------|------|------|
| typecheck | `typecheck/*.rs` | ~3,300（原 4,931） |
| semantic | `semantic/mod.rs` + tests | ~1,100 + 2,481 |
| codegen/cxx_ir | `cxx_ir/*.rs` | ~3,766 |
| codegen/llvm | `llvm/*.rs` | ~4,178 |
| codegen/mvm | `mvm.rs` | 1,629 |
| vm | `vm/*.rs` | ~3,209 |
| 前端 parser | `parser.rs` | 2,686（最大，可豁免） |

---

## 剩余待做（按优先级）

### P0 — 仓库卫生收尾
1. **清理已签入的 build 产物**：`git rm --cached miva/test/build/** examples/*/build/**`，加入 `.gitignore`
2. **Result<String> 收敛**：`miva-vm` 和 `miva-frontend-rs` 仍有 `Result<_, String>` 接口，建议统一为结构化错误

### P1 — 代码质量
3. **parser.rs 拆分**（2,686 行，最大文件）：可考虑拆为 lexer/parser 子模块，但功能稳定可不急于动
4. **LLVM IR 文本修补**：`llvm/analyze.rs` 中靠扫描生成 IR 文本修补控制流标签的逻辑可考虑重构

### P2 — 测试覆盖
5. **mvm 后端单元测试**：目前 mvm 后端仍无独立单元测试（cxx_ir 504 个、llvm 少量）
6. **端到端测试完善**：`backend_parity.rs` 覆盖主要 examples，可补充边界场景

### 不紧急
7. stdlib 版本化方式优化（目前靠目录拷贝）
8. `miva-verify` 逻辑如需要可重新引入为独立测试 harness
