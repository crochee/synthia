## 1. Renderer: replace_first_text_anywhere helper

- [x] 1.1 Add `fn replace_first_text_anywhere(content: &mut Content, new_text: &str) -> bool` in `crates/synthia-context/src/truncate.rs` (module-private, not `pub`)
- [x] 1.2 Handle `Content::Single(ContentPart::Text(t))` → set `t.text = new_text`, return `true`
- [x] 1.3 Handle `Content::Single(ContentPart::ToolResult(tr))` → set first `ContentPart::Text.text` inside `tr.content[]` (if any), return `true` on success / `false` on no inner text
- [x] 1.4 Handle `Content::Multi(parts)` → find first `ContentPart::Text` OR first `ContentPart::ToolResult` (in that order), apply replacement, return `true` on success / `false` if neither found
- [x] 1.5 Handle all other variants (Image, ToolUse, etc.) → no-op, return `false`

## 2. Renderer: cleared-placeholder branch switch

- [x] 2.1 Replace the current `if msg.content.extract_text().is_some()` check in `truncate_messages` with a call to `replace_first_text_anywhere(&mut msg.content, &marker)`
- [x] 2.2 Verify the existing 8 cleared-placeholder tests in `truncate.rs` (lines 510-633) still pass with Shape B messages
- [x] 2.3 Confirm `set_msg_text` is **not** used in the cleared branch (D3 from design — keep it for size-based path only)

## 3. Unit tests for replace_first_text_anywhere

- [x] 3.1 Add `replace_first_text_shape_a` test: Shape A message → assert text inside `ToolResult.content[0].text` is replaced, role and `tool_use_id` preserved
- [x] 3.2 Add `replace_first_text_shape_b` test: Shape B message → assert top-level `ContentPart::Text.text` is replaced
- [x] 3.3 Add `replace_first_text_multi_mixed` test: `Content::Multi([Text, ToolResult])` → assert first Text is replaced (not the ToolResult)
- [x] 3.4 Add `replace_first_text_no_match` test: `Content::Single(ContentPart::Image(_))` → assert returns `false`, no panic, message unchanged
- [x] 3.5 Add `replace_first_text_empty_tool_result_content_returns_false` (discovered during impl): `ToolResult{content: vec![]}` → returns `false`, no panic

## 4. Integration test: production-path prune → render

- [x] 4.1 Add `pipeline_prune_then_render_shape_a` test in `crates/synthia-context/tests/compact_truncate_pipeline.rs`
- [x] 4.2 Construct 5 Shape A tool-result messages (Role::User + ContentPart::ToolResult)
- [x] 4.3 Call `prune(&mut msgs, 0)` → assert all 5 marked with `tool_result_cleared_at = Some(_)`
- [x] 4.4 Call `truncate_messages(&mut msgs, &cfg, |_| true)` with `max_bytes: 1MB` (only cleared branch fires)
- [x] 4.5 Assert all 5 messages now render the placeholder text in their first `ToolResult.content[].text` field
- [x] 4.6 Assert original `tool_use_id` and `role` fields are preserved (P8 invariant: transform, never lose)

## 5. Quality gates

- [x] 5.1 `cargo +nightly fmt --all` → no new diffs (existing repo-wide nightly diffs are out of scope, per FU.3)
- [x] 5.2 `cargo clippy -p synthia-context --all-targets --all-features --tests` → no new warnings in truncate.rs or compact_truncate_pipeline.rs (pre-existing 21 warnings in compactor.rs / compaction_service.rs / service.rs are unrelated)
- [x] 5.3 `cargo test -p synthia-context --lib truncate` → all green (existing 9 + new 5 = 14 tests)
- [x] 5.4 `cargo test -p synthia-context --test compact_truncate_pipeline` → all green (existing 6 + new 1 = 7 tests)
- [x] 5.5 `cargo test -p synthia-exec` → still green (no touched files, regression sanity)
- [x] 5.6 `cargo test -p synthia-agent --lib` → still green (no touched files, regression sanity)

## 6. Spec validation

- [x] 6.1 `openspec validate prune-renderer-shape-unification` → all green
- [x] 6.2 Verify both delta spec files (`prune-idempotent-marker` and `tool-output-truncate`) parse without errors
- [x] 6.3 Verify each MODIFIED Requirement has at least one `#### Scenario:` (4 hashtags, not 3)

## 7. Commit and archive

- [x] 7.1 `git add` all modified files (truncate.rs, compact_truncate_pipeline.rs)
- [x] 7.2 Commit with message: `fix(context): renderer honors tool_result_cleared_at for ContentPart::ToolResult shape` → commit `e93b838`
- [x] 7.3 Run `openspec archive prune-renderer-shape-unification --yes` → archived as `2026-06-12-prune-renderer-shape-unification`
- [x] 7.4 Verify 2 specs synced to `openspec/specs/` baseline (`prune-idempotent-marker` + `tool-output-truncate` each +1 added)
- [x] 7.5 Write retrospective.md (1-2 pages: deviations, lessons, follow-up confirmation that FU.6 stays deferred)
- [x] 7.6 Final `git status` → working tree clean
