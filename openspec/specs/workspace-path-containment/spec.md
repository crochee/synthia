# workspace-path-containment Specification

## Purpose
TBD - created by archiving change adversarial-audit-p0-fixes. Update Purpose after archive.
## Requirements
### Requirement: File Tool Path Validation Must Use Workspace Containment

文件工具（read_file / write_file / apply_patch / search_files）的路径校验 MUST 使用纯路径 `normalize()` + workspace 前缀包含判定，禁止仅用 `../` 或 `..\\` 子串匹配作为唯一防线。

#### Scenario: Absolute Path Outside Workspace Denied

- **WHEN** 工具调用 `read_file("/etc/passwd")` 或 `read_file("/home/victim/.ssh/id_rsa")`
- **THEN** 路径校验 MUST Deny，错误信息指明路径不在 workspace 内

#### Scenario: Relative Traversal Denied

- **WHEN** 工具调用 `read_file("../../../etc/passwd")` 或 `write_file("../../../home/victim/.bashrc", ...)`
- **THEN** 路径校验 MUST Deny

#### Scenario: Workspace Internal Path Allowed

- **WHEN** 工具调用 `read_file("src/main.rs")` 或 `write_file("src/utils.rs", content)`（workspace 内合法路径）
- **THEN** 路径校验 MUST Allow

#### Scenario: Symbolic Link Not Resolved

- **WHEN** 路径校验执行 `normalize()`
- **THEN** 校验 MUST NOT 调用 `canonicalize()` 或任何会触发文件系统 I/O 的方法，以防 TOCTOU 竞态

#### Scenario: Normalize Handles Dot Segments

- **WHEN** 路径为 `workspace/src/../src/main.rs`
- **THEN** `normalize()` MUST 折叠为 `workspace/src/main.rs` 并通过前缀包含校验

