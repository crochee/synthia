<!--
Raw capture of brainstorming output.

本檔原樣捕捉 brainstorming 的產出，不強制結構。
design.md 從本檔萃取並重新整理為結構化設計文件。
-->

# tool-output-offloading Brainstorm

## Background

Synthia 的 AI agent 在本地 CLI 场景下运行时，工具输出（尤其是 `bash`、`grep`、`read` 大文件）会直接进入 LLM 上下文。随着轮次增加，这些输出会：

1. 快速耗尽上下文预算，触发 compaction/pruning
2. 破坏 prefix 稳定性（Synthia 的核心优势之一），降低 KV Cache 命中率
3. 让模型被噪声淹没，降低后续决策质量

对比生产级实现：

- **OpenCode** 有 `ToolOutputStore`：超过 `MAX_BYTES=50KB` / `MAX_LINES=2000` 的输出写入文件系统，上下文只保留摘要和路径，保留 7 天。
- **Codex** 有类似的输出管理与上下文归一化机制。
- **Synthia** 目前主要依赖 `truncate_output` 做截断，但截断后的内容仍塞进上下文。

## Decision Chain

### Q1: 产品定位是什么？
**Answer**: 本地 CLI 助手优先，后续演进为多租户生产服务。
**Implication**: 方案必须先在单用户本地环境验证，但架构要为后续多租户扩展预留空间。

### Q2: 是否接受真实沙箱作为默认？
**Answer**: 接受。
**Implication**: 本 change 不直接做沙箱，但输出旁路要与未来沙箱路径兼容（文件写入位置、权限、审计）。

### Q3: 是否希望引入规划状态机？
**Answer**: 希望。
**Implication**: 输出旁路是规划状态机的前提——减少上下文噪音后，规划模型才能聚焦。

### Q4: 是否采用 OpenCode 的输出阈值（50KB / 2000 行 / 7 天）？
**Answer**: 是。
**Implication**: 直接借用经过验证的参数，减少决策成本。

### Q5: 会话存储是否保持现有抽象？
**Answer**: 保持抽象。
**Implication**: 输出旁路存储独立实现，不依赖 session store 的具体后端。

## Design Trade-offs

### Approach A: 顺序 MVP（推荐）
1. 工具输出旁路存储（低改动、高 ROI）
2. 真实沙箱默认
3. Guardian LLM 审查
4. 规划状态机

**Pros**: 每步可独立验证；保护 prefix 稳定性；符合“先修 bug 再抽象”原则。  
**Cons**: 规划能力晚上线。

### Approach B: 架构先行
先做 `Turn` 模型 + 规划状态机，再嫁接其他功能。

**Pros**: 后续功能有统一底座。  
**Cons**: 改动面大、验证周期长；与当前项目节奏冲突。

### Approach C: 并行工作流
四个 change 同时推进。

**Pros**: 快。  
**Cons**: 本地 CLI 团队规模下 review/集成负担重。

## Chosen Approach

**Approach A**，从 `tool-output-offloading` 开始。

Rationale:
- 直接保护 Synthia 的 prefix 稳定性优势
- 不改变公共 API，风险最低
- 为后续 compaction 和规划状态机减少上下文噪音
- 可复用项目 memory 中要求的 `synthia_context::truncate::truncate_output`

## Open Threads

- 输出文件路径：放在 `~/.synthia/tool-output/` 还是项目本地 `.synthia/tool-output/`？
- 是否让模型显式请求“读取完整输出”作为一个 tool？
- 与现有 `truncate_output` 的关系：是替换、包装还是并存？
