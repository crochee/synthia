# Tasks: trait-abstraction-review

## 1. Phase 1 - 采集脚本

- [ ] 1.1 创建 `openspec/changes/2026-06-15-trait-abstraction-review/scripts/extract_trait_signals.sh`
- [ ] 1.2 实现 8 信号采集逻辑 (impl_count, method_count, generic_params, lifetime_params, associated_types, call_sites, dyn_usage, file_size_lines)
- [ ] 1.3 输出 `artifacts/trait-inventory.md` 表 (header + 57 行)
- [ ] 1.4 self-test: 准备 1-2 个 fixture trait 块 (`scripts/fixtures/`), 验证列数与数值
- [ ] 1.5 脚本 chmod +x, 跑一次 clean, 跑一次 synthetic drift, 确认输出稳定

## 2. Phase 2 - 全量扫描

- [ ] 2.1 跑 `extract_trait_signals.sh`, 生成 `artifacts/trait-inventory.md`
- [ ] 2.2 验证: 57 行 + 8 列齐全, 无空值
- [ ] 2.3 spot-check 5 个已知 trait (如 `Provider`, `PromptSection`), 对照实际 impl/calls 数量

## 3. Phase 3 - 决策矩阵分流

- [ ] 3.1 在 `artifacts/trait-inventory.md` 末尾追加 决策矩阵 + 分类列 (KEEP/REVIEW/REMOVE_CANDIDATE)
- [ ] 3.2 输出 `artifacts/deep-review-candidates.md`, 列出 10-15 个待深 review 的 trait
- [ ] 3.3 验证: 3 类加和 = 57

## 4. Phase 4 - 深度 review (10-15 个)

- [ ] 4.1 对每个候选 trait 写 `artifacts/deep-reviews/{NN}-{name}.md`
- [ ] 4.2 严格按 4 段模板: 目的 / 价值 / 替代方案 / 推荐 + 理由
- [ ] 4.3 每篇带 4-party 检查 (≥ 3 派同意)
- [ ] 4.4 决策不一致时, 写 `artifacts/disagreements.md` 留痕

## 5. Phase 5 - 汇总

- [ ] 5.1 写 `artifacts/recommendations.md`: 三类总数 + 每类典型代表
- [ ] 5.2 末尾留 "Future refactor candidates" 索引段 (P0/P1/P2)
- [ ] 5.3 验证: KEEP + REVIEW + REMOVE_CANDIDATE 加和 = 57

## 6. Phase 6 - 4-party 对抗

- [ ] 6.1 对整个 report 走 4-party 审查
- [ ] 6.2 共识 ≥ 3 派 → 接受当前 recommendations
- [ ] 6.3 分歧 → 写 `artifacts/disagreements.md`, 标"留待未来决策"

## 7. Phase 7 - 验收

- [ ] 7.1 写 `verify.md`: 列出 7 阶段实际产出 + 自检结果
- [ ] 7.2 `openspec validate 2026-06-15-trait-abstraction-review` 通过
- [ ] 7.3 commit + 推送 (按用户指令)
- [ ] 7.4 (可选) 用 `scripts/check_synced_spec_format.sh` 验证 spec 格式

## 8. 质量门

- [ ] 8.1 零新依赖 (脚本只用 `rg` + `bash` + `awk`)
- [ ] 8.2 `src/` 0 改动
- [ ] 8.3 `cargo test --workspace` 0 regression (本 change 不改 src, 应自动通过)
- [ ] 8.4 采集脚本 self-test 双路径 (clean + synthetic drift) 均通过
