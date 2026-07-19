## 1. Fix examples compilation errors

- [x] 1.1 Fix tool_usage.rs - remove RegisterableTool import, use Arc<dyn Tool> directly
- [x] 1.2 Fix tool_usage.rs - change register_tool() to register()
- [x] 1.3 Fix tool_usage.rs - handle run_with_context returning Vec<ToolOutput>
- [x] 1.4 Fix basic_chat.rs - remove unused AgentRunConfig import
- [x] 1.5 Verify examples compile with cargo build --examples

## 2. Add synthia-evaluation smoke test

- [x] 2.1 Create tests directory in synthia-evaluation if not exists
- [x] 2.2 Add basic smoke test that verifies module loads correctly
- [x] 2.3 Run cargo test -p synthia-evaluation to verify test passes

## 3. Final verification

- [x] 3.1 Run cargo build to ensure all crates compile
- [x] 3.2 Run cargo test to ensure all tests pass