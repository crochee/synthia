# Prune-Renderer Shape Unification Implementation Plan

> **For agentic workers:** Use TDD per task; the integration test in task 4 is the acceptance test for the whole change.

**Goal:** Close the gap where `prune()`-marked messages with `ContentPart::ToolResult` content are not replaced with a placeholder by `truncate_messages`.

**Architecture:** Add a module-private `replace_first_text_anywhere(&mut Content, &str) -> bool` helper in `crates/synthia-context/src/truncate.rs` that dispatches replacement by content variant. Switch the cleared-placeholder branch in `truncate_messages` to call this helper. No changes to `is_tool_result`, `prune()`, or the `Message` struct.

**Tech Stack:** Rust (Cargo), `synthia-provider` (Content/ContentPart enums), `chrono` (DateTime<Utc>), `#[serde(default)]`.

---

## Task 1: replace_first_text_anywhere helper

**File:** `crates/synthia-context/src/truncate.rs`

- [ ] **Step 1.1:** Add the function skeleton at the bottom of the impl block (just before `set_msg_text`):
  ```rust
  /// Replace the first text-like field in `content` with `new_text`.
  ///
  /// Handles both tool-result on-the-wire shapes:
  /// - `Content::Single(ContentPart::Text(t))` → set `t.text`
  /// - `Content::Single(ContentPart::ToolResult(tr))` → set first text in `tr.content[]`
  /// - `Content::Multi(parts)` → find first Text or ToolResult part
  ///
  /// Returns `true` if a replacement was made; `false` if no text-like
  /// field was found (caller treats this as a no-op).
  fn replace_first_text_anywhere(
      content: &mut Content,
      new_text: &str,
  ) -> bool {
      todo!()
  }
  ```
- [ ] **Step 1.2:** Run `cargo test -p synthia-context --lib truncate` → expect compile error (todo!() panics) but existing 9 tests should still pass since the helper isn't called yet. If any test panics, you have an ordering issue; check that you added the helper **after** the existing code, not inserted inline.

---

## Task 2: Implement the four content-variant branches

**File:** `crates/synthia-context/src/truncate.rs`

- [ ] **Step 2.1:** Replace the `todo!()` body with the match:
  ```rust
  fn replace_first_text_anywhere(
      content: &mut Content,
      new_text: &str,
  ) -> bool {
      use synthia_provider::ContentPart;
      match content {
          Content::Single(part) => match part {
              ContentPart::Text(t) => {
                  t.text = new_text.to_string();
                  true
              }
              ContentPart::ToolResult(tr) => {
                  replace_first_in_tool_result(tr, new_text)
              }
              _ => false,
          },
          Content::Multi(parts) => {
              for part in parts.iter_mut() {
                  match part {
                      ContentPart::Text(t) => {
                          t.text = new_text.to_string();
                          return true;
                      }
                      ContentPart::ToolResult(tr) => {
                          if replace_first_in_tool_result(tr, new_text) {
                              return true;
                          }
                          // continue to next part if this ToolResult had no inner text
                      }
                      _ => {}
                  }
              }
              false
          }
      }
  }

  /// Helper: replace the first Text inside `tr.content[]` if any.
  /// Returns true on success, false on empty ToolResult.content.
  fn replace_first_in_tool_result(
      tr: &mut synthia_provider::ToolResult,
      new_text: &str,
  ) -> bool {
      use synthia_provider::ContentPart;
      for part in tr.content.iter_mut() {
          if let ContentPart::Text(t) = part {
              t.text = new_text.to_string();
              return true;
          }
      }
      false
  }
  ```
- [ ] **Step 2.2:** Run `cargo build -p synthia-context` → must compile. If `ToolResult` is not exported, add `use synthia_provider::ToolResult;` at the top of the file.

---

## Task 3: Switch the cleared-placeholder branch

**File:** `crates/synthia-context/src/truncate.rs` (in `truncate_messages`)

- [ ] **Step 3.1:** Find the existing cleared-placeholder block (lines ~194-201):
  ```rust
  let cleared_at = msg.tool_result_cleared_at;
  if let Some(at) = cleared_at {
      if msg.content.extract_text().is_some() {
          let marker = cleared_placeholder(at);
          set_msg_text(msg, &marker);
      }
      continue;
  }
  ```
- [ ] **Step 3.2:** Replace with:
  ```rust
  let cleared_at = msg.tool_result_cleared_at;
  if let Some(at) = cleared_at {
      let marker = cleared_placeholder(at);
      // No-op (return false) if no text-like field exists.
      // `set_msg_text` is intentionally not used here because it
      // only handles top-level ContentPart::Text and would miss
      // the ContentPart::ToolResult shape that `prune()` marks.
      replace_first_text_anywhere(&mut msg.content, &marker);
      continue;
  }
  ```
- [ ] **Step 3.3:** Run `cargo test -p synthia-context --lib truncate` → existing 9 tests should still pass (Shape B path goes through Multi branch with ContentPart::Text).

---

## Task 4: Add unit tests for replace_first_text_anywhere

**File:** `crates/synthia-context/src/truncate.rs` (in `mod tests`)

- [ ] **Step 4.1:** Add 4 tests inside the `mod tests` block (after line 633, the last existing test):
  ```rust
  // =========================================================================
  // replace_first_text_anywhere — P0 spec: prune-renderer-shape-unification
  // =========================================================================

  fn shape_a_tool_result_msg(id: &str, body: &str) -> Message {
      Message {
          role: Role::User,
          content: Content::Single(ContentPart::ToolResult(ToolResult {
              tool_use_id: id.to_string(),
              content: vec![ContentPart::Text(TextContent {
                  text: body.to_string(),
              })],
              structured_content: None,
              is_error: None,
          })),
          ..Default::default()
      }
  }

  fn shape_b_tool_msg(id: &str, body: &str) -> Message {
      Message {
          role: Role::Tool,
          content: Content::Multi(vec![ContentPart::Text(TextContent {
              text: body.to_string(),
          })]),
          tool_call_id: Some(id.to_string()),
          ..Default::default()
      }
  }

  #[test]
  fn replace_first_text_shape_a() {
      let mut msg = shape_a_tool_result_msg("t-1", "ORIGINAL");
      let mut content = std::mem::replace(
          &mut msg.content,
          synthia_provider::Content::text(""),
      );
      let replaced = replace_first_text_anywhere(&mut content, "REPLACED");
      assert!(replaced, "Shape A must report a replacement");
      let text = match &content {
          Content::Single(ContentPart::ToolResult(tr)) => tr.content[0]
              .text()
              .unwrap()
              .to_string(),
          _ => panic!("expected ToolResult"),
      };
      assert_eq!(text, "REPLACED");
  }

  #[test]
  fn replace_first_text_shape_b() {
      let mut msg = shape_b_tool_msg("t-1", "ORIGINAL");
      let mut content = std::mem::replace(
          &mut msg.content,
          synthia_provider::Content::text(""),
      );
      let replaced = replace_first_text_anywhere(&mut content, "REPLACED");
      assert!(replaced);
      assert_eq!(content.extract_text().unwrap(), "REPLACED");
  }

  #[test]
  fn replace_first_text_multi_mixed() {
      let mut content = Content::Multi(vec![
          ContentPart::Text(TextContent { text: "FIRST".to_string() }),
          ContentPart::ToolResult(ToolResult {
              tool_use_id: "t-1".to_string(),
              content: vec![ContentPart::Text(TextContent {
                  text: "SECOND".to_string(),
              })],
              structured_content: None,
              is_error: None,
          }),
      ]);
      let replaced = replace_first_text_anywhere(&mut content, "REPLACED");
      assert!(replaced);
      // First Text replaced
      if let Content::Multi(parts) = &content {
          if let ContentPart::Text(t) = &parts[0] {
              assert_eq!(t.text, "REPLACED");
          } else {
              panic!("expected Text at index 0");
          }
          // ToolResult untouched
          if let ContentPart::ToolResult(tr) = &parts[1] {
              assert_eq!(tr.content[0].text().unwrap(), "SECOND");
          } else {
              panic!("expected ToolResult at index 1");
          }
      }
  }

  #[test]
  fn replace_first_text_no_match_image_content() {
      use synthia_provider::ImageContent;
      let mut content = Content::Single(ContentPart::Image(ImageContent {
          data: "BASE64DATA".to_string(),
          mime_type: "image/png".to_string(),
      }));
      let replaced = replace_first_text_anywhere(&mut content, "REPLACED");
      assert!(!replaced, "Image content must report no replacement");
      // Content unchanged
      if let Content::Single(ContentPart::Image(img)) = &content {
          assert_eq!(img.data, "BASE64DATA");
      } else {
          panic!("expected Image content");
      }
  }
  ```
- [ ] **Step 4.2:** Add necessary imports to the test module (top of `mod tests`):
  ```rust
  use synthia_provider::{Content, ContentPart, Role, TextContent, ToolResult};
  ```
  (If the existing test module already imports some of these, only add what's missing.)
- [ ] **Step 4.3:** Run `cargo test -p synthia-context --lib truncate` → expect 9 existing + 4 new = 13 tests, all green.

---

## Task 5: Add production-path integration test

**File:** `crates/synthia-context/tests/compact_truncate_pipeline.rs`

- [ ] **Step 5.1:** Add the new test at the bottom of the file (after the last test):
  ```rust
  /// Full production-path coverage: Shape A tool-result → prune() marks it
  /// → truncate_messages replaces with placeholder. This is the test
  /// that the previous change's reviewer caught as missing.
  #[test]
  fn pipeline_prune_then_render_shape_a() {
      use synthia_context::truncate::truncate_messages;
      use synthia_provider::{
          Content, ContentPart, Message, Role, TextContent, ToolResult,
      };

      // 5 Shape A tool-result messages (Role::User + ContentPart::ToolResult).
      let huge = "x".repeat(8_000);
      let mut msgs: Vec<Message> = (0..5)
          .map(|i| {
              Message {
                  role: Role::User,
                  content: Content::Single(ContentPart::ToolResult(
                      ToolResult {
                          tool_use_id: format!("t-{i}"),
                          content: vec![ContentPart::Text(TextContent {
                              text: huge.clone(),
                          })],
                          structured_content: None,
                          is_error: None,
                      },
                  )),
                  ..Default::default()
              }
          })
          .collect();

      // Step 1: prune with zero budget marks all 5.
      let stats = prune(&mut msgs, 0);
      assert_eq!(stats.marked_count, 5);
      for m in &msgs {
          assert!(m.tool_result_cleared_at.is_some());
      }

      // Step 2: truncate_messages with very high max_bytes so only the
      // cleared branch fires (no size-based truncation).
      let cfg = TruncateConfig {
          max_bytes: 1024 * 1024,
          head_lines: 1,
          tail_lines: 1,
          temp_dir: std::env::temp_dir().join("synthia-pipeline-it"),
      };
      let results = truncate_messages(&mut msgs, &cfg, |_| true);
      // No TruncatedResult produced — the cleared branch short-circuits.
      assert_eq!(results.len(), 0);

      // Step 3: every message must show the placeholder inside its
      // ToolResult.content[0].text, and the on-the-wire fields must
      // be preserved.
      for (i, m) in msgs.iter().enumerate() {
          let tr = match &m.content {
              Content::Single(ContentPart::ToolResult(tr)) => tr,
              _ => panic!("message {i} must remain a ToolResult"),
          };
          assert_eq!(tr.tool_use_id, format!("t-{i}"));
          let text = tr.content[0].text().unwrap();
          assert!(
              text.contains("Old tool result content cleared at"),
              "message {i} must render placeholder, got: {text:?}"
          );
          assert!(
              !text.contains(&"x".repeat(64)),
              "message {i} must not leak original body"
          );
      }
  }
  ```
- [ ] **Step 5.2:** Run `cargo test -p synthia-context --test compact_truncate_pipeline` → expect existing 6 + new 1 = 7 tests, all green.

---

## Task 6: Quality gates

- [ ] **Step 6.1:** `cargo +nightly fmt --all -- crates/synthia-context/src/truncate.rs crates/synthia-context/tests/compact_truncate_pipeline.rs` — only the new files. Skip if nightly diffs are pervasive (FU.3).
- [ ] **Step 6.2:** `cargo clippy -p synthia-context --all-targets --all-features --tests 2>&1 | grep -E "warning|error"` → no new warnings introduced by this change. Pre-existing warnings are out of scope.
- [ ] **Step 6.3:** `cargo test -p synthia-exec -p synthia-context` → all green.
- [ ] **Step 6.4:** `cargo test -p synthia-agent --lib` → all green (regression sanity for `Message::tool()` callers).

---

## Task 7: Spec validation

- [ ] **Step 7.1:** `openspec validate prune-renderer-shape-unification` → all green.
- [ ] **Step 7.2:** Manually inspect `openspec/changes/prune-renderer-shape-unification/specs/prune-idempotent-marker/spec.md` — confirm 4 `#### Scenario:` entries under the new Requirement, each with `**WHEN**` and `**THEN**`.
- [ ] **Step 7.3:** Manually inspect `openspec/changes/prune-renderer-shape-unification/specs/tool-output-truncate/spec.md` — confirm 5 `#### Scenario:` entries.

---

## Task 8: Commit

- [ ] **Step 8.1:** `git add crates/synthia-context/src/truncate.rs crates/synthia-context/tests/compact_truncate_pipeline.rs`
- [ ] **Step 8.2:** Commit message:
  ```
  fix(context): renderer honors tool_result_cleared_at for ToolResult shape

  Closes FU.1 from compact-truncate-prune-convergence retrospective.
  truncate_messages cleared-placeholder branch now uses a new
  private helper `replace_first_text_anywhere` that drills into
  ContentPart::ToolResult.content[].text for the Anthropic / OpenAI
  on-the-wire shape (the shape that `prune()` actually marks).

  The helper also handles the legacy Role::Tool + ContentPart::Text
  shape, so all existing tests keep passing unchanged. The original
  extract_text()-based gate is removed because it would silently
  miss the ToolResult shape that production pruner marks.

  Adds 4 unit tests for the helper and 1 production-path integration
  test that exercises prune() → truncate_messages → placeholder on
  Shape A messages (the gap that the previous change exposed).
  ```

---

## Risk register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| `set_msg_text` accidentally still used in cleared branch | Low | Low (D3 in design) | Code review step 3.3; existing tests catch regression |
| `ToolResult.content` is empty → no replacement | Medium | Low (no-op is correct) | Test 4.4 covers Image (no text) returns false |
| `ToolResult` not exported from `synthia_provider` | Low | Low | Build error → add `use` import in step 2.2 |
| Multi-part content with only Image/ToolUse (no Text) | Low | Low | No-op branch in test 4.4 |

## Out-of-scope confirmation

- **FU.6** (auto-invoke `prune()` in `StepCompact`): confirmed deferred. Production loop does not push tool results into `ctx.messages`, so wiring now scans an empty list. Lands with the future change that promotes tool results into `ctx.messages`.
- **FU.2** (pre-computed tokens in `compact_with_fallback`): unchanged.
- **FU.3** (rustfmt baseline): unchanged.
- **FU.4** (`lifecycle_tools.rs` < 300 LoC): unchanged.
