<!--
Raw capture of superpowers:brainstorming output.

本檔原樣捕捉 brainstorming skill 的產出，不強制結構。
Skill 的自然產出通常是 decision log 格式（背景 → 決議鏈 Q1-Qn → 設計取捨），
但依對話內容可能有不同組織方式。

design.md 從本檔萃取並重新整理為結構化設計文件。

不要將本檔的內容複製到 design.md — design.md 是獨立的重組產物，
兩者互補但不重疊。
-->

# Brainstorming: Synthia 项目优化方向

## 背景 (Context)

Synthia 是一个模块化的 Rust AI Agent 框架，当前状态：
- 22-crate workspace，主要 crates 已构建成功
- 最近完成大规模重构：task system、permission system、删除了 multi-agent
- 存在 2 个 clippy lint 错误阻塞 CI
- `synthia-tool/registry.rs` 1193 行，代码质量存在问题
- 性能方面：memory cold storage (sqlx)、embedding 计算、build time 可优化

## 探索过程 (Exploration)

### 问题识别
通过 `cargo clippy --workspace` 发现 2 个 lint 错误在 `synthia-agent/src/agent_tools.rs`
通过 `cargo build --examples` 确认基本构建正常
通过 git diff 分析近 10 次 commits 的重构范围

### 澄清问题
Q1: 用户想要优化哪些方面？
A: 用户表示全部都需要（clippy fixes、code quality、performance、architecture review）

### Approach 选择
提出 3 个方案：

**A) 串行方案**
按依赖顺序逐个处理：Clippy → Code quality → Performance + Architecture

**B) 并行流方案（推荐）**
分 3 条独立流并行执行：
- Stream 1: 质量清理（clippy + 大文件拆分）
- Stream 2: 架构审查（permission 重构后结构 + multi-agent 残留引用）
- Stream 3: 性能优化（build time、memory、embedding）

**C) 一次性方案**
先全面 audit，再出一份完整优化 spec

**用户选择：B（并行流方案）**

## 设计决策 (Design Decision)

### 推荐方案：B 并行流方案

**理由：**
1. Clippy 是 blocker，独立快速修复
2. 大文件拆分和性能优化可以并行探索
3. 架构 review 结论可以指导其他 stream 的优先级

### 三条 Stream 设计

```
Stream 1: 质量清理
├── 1. Fix clippy errors (agent_tools.rs)
├── 2. Split synthia-tool/registry.rs (1193 → ?)
└── 3. Dead code 清理

Stream 2: 架构审查
├── 1. 审查 permission system 重构后结构
├── 2. 检查 multi-agent 删除后的残留引用
└── 3. 评估 task/scheduler 职责边界

Stream 3: 性能优化
├── 1. Profile build time (sccache, parallelization)
├── 2. Analyze memory cold storage (sqlx)
└── 3. 检查 embedding 计算瓶颈
```

### 关键约束
- 三条流独立，无共享状态
- Stream 2 输出影响 Stream 1/3 的优先级
- 最终合并为一个 PR 或分批合并

## 输出 (Output)

**Design 已获用户批准**，方向确认为并行流方案。

后续步骤：
1. 创建 design.md（重组本档案为结构化设计）
2. 创建 proposal.md
3. 创建 specs（架构审查spec、性能优化spec等）
4. 创建 tasks.md（实施步骤）
5. 创建 plan.md
6. 创建 verify.md + retrospective