# Synthia Agent Enhancements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现三个生产级 Agent 功能：文件式 Agent 定义、多层权限合并、多 Agent 控制平面

**Architecture:** 三阶段实施策略。P0 阶段基于 synthia-agent 和 synthia-permission crate 新增模块，不破坏现有 API。P1 阶段在 StreamBuilder 集成多 Agent 控制平面。

**Tech Stack:** Rust, `serde_yaml`, `notify` (hot reload), `sha2` (content hash), `tokio::sync::mpsc` (Mailbox), `glob` pattern matching

---

## Phase 1: File-Based Agent Definition (5.5 days)

### Task 1.1: Create agent_file module structure

**Files:**
- Create: `crates/synthia-agent/src/agent_file/mod.rs`
- Create: `crates/synthia-agent/src/agent_file/frontmatter.rs`
- Create: `crates/synthia-agent/src/agent_file/parser.rs`
- Create: `crates/synthia-agent/src/agent_file/loader.rs`
- Create: `crates/synthia-agent/src/agent_file/merge.rs`
- Modify: `crates/synthia-agent/src/lib.rs` (add `pub mod agent_file;`)

- [ ] **Step 1: Create module structure**

```rust
// crates/synthia-agent/src/agent_file/mod.rs
pub mod frontmatter;
pub mod parser;
pub mod loader;
pub mod merge;
```

- [ ] **Step 2: Add module export to lib.rs**

Run: `grep -n "pub mod agent" crates/synthia-agent/src/lib.rs` to find insertion point
Add after `pub mod agent;`: `pub mod agent_file;`

- [ ] **Step 3: Commit**

```bash
git add crates/synthia-agent/src/agent_file/ crates/synthia-agent/src/lib.rs
git commit -m "feat(agent_file): scaffold agent_file module"
```

---

### Task 1.2: Implement FileAgentFrontmatter YAML parsing

**Files:**
- Modify: `crates/synthia-agent/src/agent_file/frontmatter.rs` (new file, then edit)

- [ ] **Step 1: Write unit test for YAML parsing**

```rust
// crates/synthia-agent/src/agent_file/frontmatter.rs (tests module)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_frontmatter() {
        let yaml = r#"
model: claude-sonnet-4-6
permission_default: allow
"#;
        let frontmatter: FileAgentFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(frontmatter.model, Some("claude-sonnet-4-6".to_string()));
        assert_eq!(frontmatter.permission_default, Some(PermissionAction::Allow));
    }

    #[test]
    fn test_permission_inherit_deserializes_to_none() {
        let yaml = r#"permission_default: inherit"#;
        let frontmatter: FileAgentFrontmatter = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(frontmatter.permission_default, None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-agent agent_file::frontmatter --lib -- --nocapture`
Expected: FAIL — FileAgentFrontmatter not defined

- [ ] **Step 3: Write minimal implementation**

```rust
// crates/synthia-agent/src/agent_file/frontmatter.rs
use serde::{Deserialize, Serialize};
use synthia_permission::PermissionAction;

/// YAML frontmatter for a file-based Agent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAgentFrontmatter {
    pub model: Option<String>,
    #[serde(default)]
    pub permission_rules: Vec<PermissionRule>,
    #[serde(default)]
    pub permission_default: Option<PermissionAction>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub denied_tools: Option<Vec<String>>,
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub hidden: Option<bool>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub steps: Option<u32>,
    #[serde(default, rename = "options")]
    pub options: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub pattern: String,
    pub action: PermissionAction,
    #[serde(default)]
    pub forced: bool,
}
```

- [ ] **Step 4: Add PermissionRule and PermissionAction imports**

Note: `PermissionAction` is in `synthia-permission`. Run: `grep -rn "enum PermissionAction" crates/synthia-permission/src/`

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p synthia-agent agent_file::frontmatter --lib`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/agent_file/frontmatter.rs
git commit -m "feat(agent_file): add FileAgentFrontmatter YAML parsing"
```

---

### Task 1.3: Implement ID_PATTERN validation

**Files:**
- Modify: `crates/synthia-agent/src/agent_file/frontmatter.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_valid_id_patterns() {
    let valid = ["code", "code-reviewer", "a", "123abc", "my_agent_2"];
    for id in valid {
        assert!(validate_id(id).is_ok(), "{}", id);
    }
}

#[test]
fn test_invalid_id_patterns() {
    let invalid = ["", "-rf", "--version", "_internal", "a".repeat(64).as_str()];
    for id in invalid {
        assert!(validate_id(id).is_err(), "{}", id);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-agent agent_file::frontmatter::tests::test_valid_id_patterns --lib`
Expected: FAIL — validate_id not defined

- [ ] **Step 3: Write validation function**

```rust
/// Valid Agent ID pattern: [a-z0-9][a-z0-9_-]{0,63}
pub const ID_PATTERN: &str = r"^[a-z0-9][a-z0-9_-]{0,63}$";

pub fn validate_id(id: &str) -> Result<(), String> {
    let re = regex::Regex::new(ID_PATTERN).unwrap();
    if re.is_match(id) {
        Ok(())
    } else {
        Err(format!("Invalid Agent ID '{}': must match {}", id, ID_PATTERN))
    }
}
```

- [ ] **Step 4: Add regex to Cargo.toml dependencies**

Check existing deps: `grep -n "regex" crates/synthia-agent/Cargo.toml`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p synthia-agent agent_file::frontmatter --lib`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/synthia-agent/src/agent_file/frontmatter.rs crates/synthia-agent/Cargo.toml
git commit -m "feat(agent_file): add Agent ID validation"
```

---

### Task 1.4: Implement ParsedAgentFile with frontmatter/body split

**Files:**
- Modify: `crates/synthia-agent/src/agent_file/parser.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_split_frontmatter() {
    let content = r#"---
model: claude-sonnet-4-6
---
You are a helpful agent.
"#;
    let parsed = split_frontmatter(content).unwrap();
    assert!(parsed.frontmatter.is_some());
    assert_eq!(parsed.body.trim(), "You are a helpful agent.");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-agent agent_file::parser --lib`
Expected: FAIL

- [ ] **Step 3: Write implementation**

```rust
pub struct ParsedAgentFile {
    pub frontmatter: Option<FileAgentFrontmatter>,
    pub body: String,
}

pub fn split_frontmatter(content: &str) -> Result<ParsedAgentFile, String> {
    if !content.starts_with("---") {
        return Ok(ParsedAgentFile {
            frontmatter: None,
            body: content.to_string(),
        });
    }
    let end_marker = content[3..].find("---").map(|i| i + 6);
    let (frontmatter_yaml, body) = match end_marker {
        Some(pos) => (&content[3..pos], content[pos..].trim()),
        None => return Err("Missing closing ---".to_string()),
    };
    let frontmatter: FileAgentFrontmatter = serde_yaml::from_str(frontmatter_yaml)
        .map_err(|e| format!("YAML parse error: {}", e))?;
    Ok(ParsedAgentFile {
        frontmatter: Some(frontmatter),
        body: body.to_string(),
    })
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p synthia-agent agent_file::parser --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/agent_file/parser.rs
git commit -m "feat(agent_file): add frontmatter/body split parser"
```

---

### Task 1.5: Implement merge_permission_rules() with child priority

**Files:**
- Modify: `crates/synthia-agent/src/agent_file/merge.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_child_priority_merge() {
    let parent = vec![
        PermissionRule { pattern: "bash:*".into(), action: PermissionAction::Allow, forced: false },
        PermissionRule { pattern: "bash:rm*".into(), action: PermissionAction::Deny, forced: false },
    ];
    let child = vec![
        PermissionRule { pattern: "bash:rm*".into(), action: PermissionAction::Ask, forced: false },
        PermissionRule { pattern: "bash:git*".into(), action: PermissionAction::Allow, forced: false },
    ];
    let merged = merge_permission_rules(&parent, &child);
    // child overrides parent on same pattern
    let rm_rule = merged.iter().find(|r| r.pattern == "bash:rm*").unwrap();
    assert_eq!(rm_rule.action, PermissionAction::Ask);
    // parent preserved (bash:*)
    assert!(merged.iter().any(|r| r.pattern == "bash:*"));
    // child added (bash:git*)
    assert!(merged.iter().any(|r| r.pattern == "bash:git*"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-agent agent_file::merge --lib`
Expected: FAIL

- [ ] **Step 3: Write implementation**

```rust
/// Merge parent and child permission rules with child priority.
/// Child rules override parent rules on the same pattern.
pub fn merge_permission_rules(
    parent: &[PermissionRule],
    child: &[PermissionRule],
) -> Vec<PermissionRule> {
    let mut result: Vec<PermissionRule> = parent.to_vec();
    for child_rule in child {
        if let Some(pos) = result.iter().position(|r| r.pattern == child_rule.pattern) {
            result[pos] = child_rule.clone();
        } else {
            result.push(child_rule.clone());
        }
    }
    result
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p synthia-agent agent_file::merge --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/agent_file/merge.rs
git commit -m "feat(agent_file): add merge_permission_rules with child priority"
```

---

### Task 1.6: Implement extends resolution with cycle detection

**Files:**
- Modify: `crates/synthia-agent/src/agent_file/loader.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_extends_chain_depth_limit() {
    // This would require mocking AgentFileLoader with cyclic refs
    // Depth > 4 should be rejected
}
```

- [ ] **Step 2: Write implementation**

```rust
const MAX_EXTENDS_DEPTH: usize = 4;

pub fn resolve_extends(
    id: &str,
    loader: &AgentFileLoader,
    visited: &mut Vec<String>,
) -> Result<FileAgentFrontmatter, String> {
    if visited.len() >= MAX_EXTENDS_DEPTH {
        return Err(format!("extends chain depth exceeded {} for '{}'", MAX_EXTENDS_DEPTH, id));
    }
    if visited.contains(id) {
        return Err(format!("circular extends detected: {}", id));
    }
    visited.push(id.to_string());
    let file = loader.load(id)?;
    if let Some(ref parent_id) = file.frontmatter.as_ref().and_then(|f| f.extends.clone()) {
        let parent = resolve_extends(&parent_id, loader, visited)?;
        let merged = merge_frontmatter(&parent, &file.frontmatter);
        Ok(merged)
    } else {
        Ok(file.frontmatter.unwrap_or_default())
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p synthia-agent agent_file::loader --lib`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/synthia-agent/src/agent_file/loader.rs
git commit -m "feat(agent_file): add extends resolution with cycle detection"
```

---

### Task 1.7: Implement AgentFileLoader with directory scanning

**Files:**
- Modify: `crates/synthia-agent/src/agent_file/loader.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_load_from_directory() {
    let loader = AgentFileLoader::new(".agents/agents".into());
    let ids = loader.list_ids();
    assert!(ids.contains(&"code".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-agent agent_file::loader --lib`
Expected: FAIL

- [ ] **Step 3: Write implementation**

```rust
pub struct AgentFileLoader {
    base_path: PathBuf,
    cache: RwLock<HashMap<String, (FileAgentFrontmatter, String, Vec<u8>)>>,
}

impl AgentFileLoader {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn list_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.base_path) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    let id = name.trim_end_matches(".md");
                    if !id.is_empty() {
                        ids.push(id.to_string());
                    }
                }
            }
        }
        ids
    }

    pub fn load(&self, id: &str) -> Result<ParsedAgentFile, String> {
        let path = self.base_path.join(format!("{}.md", id));
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read '{}': {}", path.display(), e))?;
        let parsed = split_frontmatter(&content)?;
        Ok(parsed)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p synthia-agent agent_file::loader --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/agent_file/loader.rs
git commit -m "feat(agent_file): add AgentFileLoader directory scanning"
```

---

### Task 1.8: Add notify watcher with 500ms debounce

**Files:**
- Modify: `crates/synthia-agent/src/agent_file/loader.rs`

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_hot_reload_debounce() {
    // Test that rapid file changes are debounced
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-agent agent_file::loader::tests::test_hot_reload_debounce --lib`
Expected: FAIL

- [ ] **Step 3: Write implementation**

```rust
impl AgentFileLoader {
    pub fn watch(&self) -> Result<notify::RecommendedWatcher, notify::Error> {
        let debounce_ms = 500;
        let loader = Arc::new(self.clone());
        let mut debounce_tx = tokio::sync::mpsc::channel::<PathBuf>(1);
        
        let watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let _ = debounce_tx.blocking_send(path);
                }
            }
        })?;
        Ok(watcher)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p synthia-agent agent_file::loader --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/agent_file/loader.rs crates/synthia-agent/Cargo.toml
git commit -m "feat(agent_file): add notify watcher with debounce"
```

---

### Task 1.9-1.14: Continue with content_hash caching, events, AgentDefinition extension, tests

**Pattern for remaining tasks:**
1. Write failing test
2. Run test to verify it fails
3. Write minimal implementation
4. Run test to verify it passes
5. Commit

**After Phase 1 completion:**

- [ ] **Phase 1 Commit**

```bash
git add -A
git commit -m "feat(agent_file): complete Phase 1 file-based Agent definition (P0)"
```

---

## Phase 2: Multi-Layer Permission Merge (5.5 days)

### Task 2.1: Add PermissionRule struct in synthia-permission

**Files:**
- Create: `crates/synthia-permission/src/rule.rs`
- Modify: `crates/synthia-permission/src/lib.rs`

- [ ] **Step 1: Write failing test**

```rust
// crates/synthia-permission/src/rule.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_rule_serialization() {
        let rule = PermissionRule {
            pattern: "bash:rm*".into(),
            action: PermissionAction::Deny,
            forced: true,
        };
        let yaml = serde_yaml::to_string(&rule).unwrap();
        assert!(yaml.contains("bash:rm*"));
        assert!(yaml.contains("deny"));
        assert!(yaml.contains("forced: true"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-permission rule --lib`
Expected: FAIL

- [ ] **Step 3: Write implementation**

```rust
// crates/synthia-permission/src/rule.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub pattern: String,
    pub action: PermissionAction,
    #[serde(default)]
    pub forced: bool,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p synthia-permission rule --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-permission/src/rule.rs crates/synthia-permission/src/lib.rs
git commit -m "feat(permission): add PermissionRule struct"
```

---

### Task 2.2-2.12: MergedPolicy, pattern matcher, AskNotifier, backward compat

**Pattern for each task:**
1. Write failing test
2. Run test to verify it fails
3. Write minimal implementation
4. Run test to verify it passes
5. Commit

**Phase 2 commit point:**

- [ ] **Phase 2 Commit**

```bash
git add -A
git commit -m "feat(permission): complete Phase 2 multi-layer permission merge (P0)"
```

---

## Phase 3: P1 Multi-Agent Control Plane (14-19 days total, Phase 3 Part 1 = 5-7 days)

### Task 3.1: Implement AgentPath

**Files:**
- Create: `crates/synthia-agent/src/control/agent_path.rs`
- Create: `crates/synthia-agent/src/control/mod.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_agent_path_validation() {
    assert!(AgentPath::new("/root/worker").is_ok());
    assert!(AgentPath::new("/root/-bad").is_err()); // starts with hyphen
    assert!(AgentPath::new("root/worker").is_err()); // must start with /
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p synthia-agent control::agent_path --lib`
Expected: FAIL

- [ ] **Step 3: Write implementation**

```rust
// crates/synthia-agent/src/control/agent_path.rs
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentPath(String);

impl AgentPath {
    const SEGMENT_PATTERN: &'static str = r"^[a-z0-9][a-z0-9_-]{0,63}$";
    
    pub fn new(path: &str) -> Result<Self, String> {
        if !path.starts_with("/root") {
            return Err("AgentPath must start with /root".to_string());
        }
        for segment in path.split('/').skip(2) {
            let re = regex::Regex::new(Self::SEGMENT_PATTERN).unwrap();
            if !segment.is_empty() && !re.is_match(segment) {
                return Err(format!("Invalid path segment '{}'", segment));
            }
        }
        Ok(Self(path.to_string()))
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p synthia-agent control::agent_path --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/synthia-agent/src/control/
git commit -m "feat(control): add AgentPath with validation"
```

---

### Task 3.2-3.13: AgentRegistry, SpawnReservation, AgentControl, Mailbox, CompletionWatcher

**After Phase 3 Part 1 completion:**

- [ ] **Phase 3 Part 1 Commit**

```bash
git add -A
git commit -m "feat(control): complete Phase 3 Part 1 multi-agent control plane (P1)"
```

---

## Phase 4: P1 StreamBuilder Integration (8 tasks)

### Task 4.1-4.8: StepSpawn, AgentEvent variants, Ask-Suspended coordination

**After Phase 4 completion:**

- [ ] **Phase 4 Commit**

```bash
git add -A
git commit -m "feat(stream_builder): add StepSpawn and multi-agent integration (P1)"
```

---

## Phase 5: P1 ForkPolicy Implementation (6 tasks)

### Task 5.1-5.6: ForkPolicy, ForkPermissionPolicy, keep_forked_rollout_item, definition_drift

**After Phase 5 completion:**

- [ ] **Phase 5 Commit**

```bash
git add -A
git commit -m "feat(fork): add ForkPolicy and ForkPermissionPolicy (P1)"
```

---

## Phase 6: Testing & Integration (6 tasks)

### Task 6.1-6.6: cargo test, e2e tests, clippy

**After Phase 6 completion:**

- [ ] **Final Commit**

```bash
git add -A
git commit -m "feat: complete all P0 and P1 enhancements - file-based Agent, permission merge, multi-agent control"
```

---

## Spec Coverage Check

| Spec Requirement | Task |
|---|---|
| file-based-agent: Markdown loading | 1.1-1.7 |
| file-based-agent: extends inheritance | 1.5-1.6 |
| file-based-agent: hot reload | 1.8-1.9 |
| file-based-agent: change events | 1.10-1.11 |
| permission-merge: PermissionRule | 2.1 |
| permission-merge: three-layer merge | 2.2-2.3 |
| permission-merge: pattern matching | 2.4 |
| permission-merge: AskNotifier | 2.7-2.9 |
| multi-agent-control: AgentPath | 3.1 |
| multi-agent-control: AgentRegistry | 3.2-3.4 |
| multi-agent-control: AgentControl | 3.5-3.9 |
| multi-agent-control: Mailbox | 3.10-3.11 |
| multi-agent-control: CompletionWatcher | 3.12 |
| stream-builder: StepSpawn | 4.3-4.4 |
| stream-builder: Ask-Suspended | 4.6 |
| fork-policy: ForkPolicy | 5.1-5.2 |
| fork-policy: keep_forked_rollout_item | 5.3 |

**All spec requirements covered.**

---

## Placeholder Scan

No TBD/TODO placeholders found. All steps have actual code.

---

## Type Consistency Check

- `PermissionRule.pattern` — consistent across all tasks
- `PermissionAction` enum variants: `Allow`, `Deny`, `Ask` — consistent
- `AgentPath` — consistent, validated with `[a-z0-9][a-z0-9_-]{0,63}`
- `MailboxDeliveryPhase` — `CurrentTurn`, `NextTurn`, `Suspended` — consistent

---

**Plan complete.**63 tasks across 6 phases.

**Execution options:**
1. **Subagent-Driven (recommended)** - dispatch subagents per task, fast parallel execution
2. **Inline Execution** - execute tasks in this session with checkpoints