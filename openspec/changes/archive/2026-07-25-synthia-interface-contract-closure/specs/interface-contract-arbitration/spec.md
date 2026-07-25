## ADDED Requirements

### Requirement: 协议源优先规则

当双侧契约存在不一致时 MUST 按以下优先级修正确认方向：(1) A2A 官方 spec（若端点属于 A2A 协议面）；(2) `@a2a-js/sdk` TypeScript 类型（前端依赖的官方客户端）；(3) Synthia 既有 `event-v2-system` / `session-replay-harness` spec 中的稳定面。任何修复卡片 MUST 注明其依据的源条款。

#### Scenario: 有官方协议源
- **WHEN** 修复卡片对应端点匹配 (1)(2)(3) 任一源
- **THEN** 卡片 MUST 在 Reason 段引用该源条款（如 "A2A v0.3 §4.2 Message.lastChunk"）

#### Scenario: 无官方协议源
- **WHEN** 修复卡片无对应 (1)(2)(3) 源
- **THEN** 卡片 MUST 标注"无协议源"并附带最小 ADR 说明，禁止"拍脑袋"修改

### Requirement: 契约修改必须满足回归保护

任何契约表行的字段重命名 / 类型变更 / 枚举值调整 MUST 同步：(a) 修复两侧实现；(b) 更新契约表；(c) 在 Playwright 契约集中加入对应正向 / 反向用例。

#### Scenario: 三同步缺失
- **WHEN** 一个修改只完成了 (a)(b)(c) 中的部分
- **THEN** 该修改 MUST 在 PR 中被标记 incomplete，并在 `verify.md` 中作为 TODO 跟踪
