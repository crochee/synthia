# Wire Tool Orchestrator into Agent Runtime — Implementation Plan

> **For agentic workers:** Use `subagent-driven-development` to implement this plan task-by-task.
> 本 plan 因可用技能列表中没有 `superpowers:writing-plans`，改以手动根据 tasks.md 与 design.md 拆解。

**Goal:** 让 `Agent::run_stream` 和 `Agent::resume` 默认启用 `DefaultToolOrchestrator`，使审批、沙箱、重试、取消等能力真正生效。

**Architecture:** 在 `Agent` 结构体上增加可选的 `approval_service`/`sandbox_manager` 字段与构造方法；在 `run_stream`/`resume` 构建 `AgentRunConfig` 时，若 `tool_orchestrator` 为 `None`，则自动调用 `build_default_tool_orchestrator()` 装配。未注入服务时使用 `HeadlessApprovalService` + `NoopSandboxManager`，保持 fail-closed 和向后兼容。

**Tech Stack:** Rust, `synthia-agent`, `synthia-tool-orchestrator`, `synthia-permission`, `synthia-sandbox`, `tokio-util`.

---

## Task 1: Agent 结构体与构造方法

**目标：** 让 `Agent` 能够持有并注入 `ApprovalService` 和 `SandboxManager`。

- [ ] **Step 1.1:** 打开 `crates/synthia-agent/src/agent.rs`，在 `AgentInitConfig` 末尾增加：
  ```rust
  pub approval_service: Option<Arc<dyn ApprovalService>>,
  pub sandbox_manager: Option<Arc<dyn SandboxManager>>,
  ```
- [ ] **Step 1.2:** 在 `Agent` 结构体末尾增加同样字段。
- [ ] **Step 1.3:** 在 `Agent::new(init: AgentInitConfig)` 中，把这两个字段从 `init` 取出；如果 `AgentInitConfig` 当前返回的是 `Self { ... }` 字面量，则添加 `approval_service: init.approval_service, sandbox_manager: init.sandbox_manager`。注意 `AgentInitConfig` 可能在别处被构造，因此新增字段必须为 `Option`，避免破坏现有调用点。
- [ ] **Step 1.4:** 实现 builder 方法：
  ```rust
  pub fn with_approval_service(mut self, service: Arc<dyn ApprovalService>) -> Self {
      self.approval_service = Some(service);
      self
  }
  pub fn with_sandbox_manager(mut self, manager: Arc<dyn SandboxManager>) -> Self {
      self.sandbox_manager = Some(manager);
      self
  }
  ```
- [ ] **Step 1.5:** 确认新增 import：`use synthia_permission::ApprovalService; use synthia_sandbox::SandboxManager;`。

**验收：** `cargo check -p synthia-agent` 通过。

---

## Task 2: 默认装配逻辑

**目标：** 在 `run_stream` 和 `resume` 中自动装配 `DefaultToolOrchestrator`。

- [ ] **Step 2.1:** 在 `Agent` impl 中新增私有辅助函数：
  ```rust
  fn assemble_default_orchestrator(
      &self,
      run_config: &mut AgentRunConfig,
  ) {
      if run_config.tool_orchestrator.is_some() {
          return;
      }
      let approval_service = self
          .approval_service
          .clone()
          .unwrap_or_else(|| Arc::new(synthia_permission::HeadlessApprovalService));
      let sandbox_manager = self
          .sandbox_manager
          .clone()
          .unwrap_or_else(|| Arc::new(synthia_sandbox::NoopSandboxManager));
      let (orchestrator, _resolver) = crate::tools::orchestrator::build_default_tool_orchestrator(
          &self.config.workspace_root,
          approval_service,
          sandbox_manager,
      );
      run_config.tool_orchestrator = Some(orchestrator);
  }
  ```
  注意：若 `NoopSandboxManager` 在 `synthia_sandbox` 中不存在，则查找现有 noop 实现或使用 `synthia_sandbox` 导出的默认实现。
- [ ] **Step 2.2:** 在 `Agent::run_stream` 中，于 `Self::run_stream_with_state(state_config)` 调用之前，对 `run_config` 调用 `self.assemble_default_orchestrator(&mut run_config)`。
- [ ] **Step 2.3:** 在 `Agent::resume` 中，于 `let state_config = ...` 之前，对 `run_config` 调用 `self.assemble_default_orchestrator(&mut run_config)`。
- [ ] **Step 2.4:** 检查 `AgentRunConfig` 的 `approval_service` 和 `sandbox_manager` 字段是否还需要保留。因为本次装配从 `Agent` 字段读取，这两个 `AgentRunConfig` 字段在 `main_loop.rs` 中仍被忽略；为保持 API 兼容可保留，但考虑是否删除以避免混淆。本次 plan 选择保留，避免扩大变更范围。

**验收：** `cargo check -p synthia-agent` 通过。

---

## Task 3: 测试

**目标：** 验证默认装配与显式注入行为。

- [ ] **Step 3.1:** 在 `crates/synthia-agent/src/agent.rs` 的 `#[cfg(test)]` 模块中新增测试：构造最小 `Agent`，调用 `run_stream`，断言生成的 `AgentRunConfig.tool_orchestrator` 为 `Some`。
  - 由于 `run_stream` 是异步且返回 stream，可改为把装配逻辑抽到可独立测试的 `assemble_default_orchestrator`（接受 `&mut AgentRunConfig`），然后直接测试该函数。
- [ ] **Step 3.2:** 新增测试：显式注入 `tool_orchestrator` 后调用 `assemble_default_orchestrator`，断言其不会被覆盖。
- [ ] **Step 3.3:** 新增测试：使用 `HeadlessApprovalService` 的 orchestrator 执行 `bash` 工具调用，断言返回 `ToolOrchestratorError::Denied`。
- [ ] **Step 3.4:** 检查 `crates/synthia-agent/src/agent.rs` 现有测试是否因新增字段而失败；若有，给 `AgentInitConfig` 的测试构造补上 `approval_service: None, sandbox_manager: None`。

**验收：** `cargo test -p synthia-agent` 通过。

---

## Task 4: 验证与清理

**目标：** 通过格式、lint、测试，并清理无用代码。

- [ ] **Step 4.1:** 运行 `cargo +nightly fmt --all`。
- [ ] **Step 4.2:** 运行 `cargo clippy --all-targets --all-features --tests --all` 并修复所有警告（包括本次新增 import 是否被使用）。
- [ ] **Step 4.3:** 运行 `cargo test` 全量测试，确认无回归。
- [ ] **Step 4.4:** 搜索新增但未使用的字段/变量/import，全部删除。严禁使用 `#[allow(dead_code)]` 或 `#[allow(unused)]`。

**验收：** `cargo fmt` + `cargo clippy` + `cargo test` 全部通过。

---

## Commit Points

1. 完成 Task 1 后可提交：`feat(agent): add approval_service and sandbox_manager injection points`
2. 完成 Task 2 后可提交：`feat(agent): auto-assemble default tool orchestrator in run_stream/resume`
3. 完成 Task 3 后可提交：`test(agent): cover default orchestrator assembly and headless denial`
4. 完成 Task 4 后可提交：`chore(agent): fmt/clippy and remove dead code`
