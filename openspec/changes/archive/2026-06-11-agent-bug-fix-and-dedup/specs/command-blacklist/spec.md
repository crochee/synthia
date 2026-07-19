# command-blacklist Specification

## Purpose

Replace the misleadingly-named `synthia_exec::sandbox::Sandbox` with `synthia_exec::command_blacklist::CommandBlacklist`. The original name falsely implied OS-level sandboxing; the actual implementation is a 25-pattern string-match blacklist. This spec makes the naming and security model honest (per security review R2/R5).

## ADDED Requirements

### Requirement: Module and type shall be renamed to command_blacklist

`synthia-exec/src/command_blacklist.rs` SHALL define `pub struct CommandBlacklist`.

The legacy `synthia-exec/src/sandbox.rs` SHALL be deleted; the `sandbox` module SHALL NOT be publicly re-exported.

A type alias `pub type Sandbox = CommandBlacklist;` SHALL be provided for **exactly 1 release** as a deprecation bridge, with `#[deprecated(note = "Use CommandBlacklist")]` attribute.

#### Scenario: CommandBlacklist is the new public type
- **WHEN** downstream code references `synthia_exec::command_blacklist::CommandBlacklist`
- **THEN** the type SHALL be `pub` and importable
- **THEN** the type SHALL be constructable with the same parameters as the old `Sandbox` struct

#### Scenario: Old sandbox module is removed
- **WHEN** the rename is complete
- **THEN** `synthia-exec/src/sandbox.rs` SHALL NOT exist
- **THEN** `pub mod sandbox;` SHALL NOT appear in `synthia-exec/src/lib.rs`
- **THEN** `grep -r "pub mod sandbox" crates/synthia-exec/` SHALL return 0 results

#### Scenario: Legacy Sandbox alias works during deprecation
- **WHEN** downstream code imports `use synthia_exec::sandbox::Sandbox;`
- **THEN** compilation SHALL succeed (the alias resolves)
- **THEN** the compiler SHALL emit a deprecation warning recommending `command_blacklist::CommandBlacklist`

---

### Requirement: Method name shall reflect blacklist semantics

`CommandBlacklist::is_command_blacklisted(&self, command: &str) -> bool` SHALL be the canonical method name.

The method SHALL return `true` if the command matches any of 25+ blacklisted patterns (case-insensitive substring or pattern match), `false` otherwise.

The doc comment SHALL explicitly state: "This is a string-match blacklist, NOT an OS-level sandbox. It does not prevent malicious commands that bypass pattern matching (e.g., unicode obfuscation, base64 encoding, `r""m"` syntax). For real sandboxing, see `synthia-sandbox-linux` (future)."

#### Scenario: Method name change
- **WHEN** `command_blacklist.is_command_blacklisted("rm -rf /")` is called
- **THEN** the return value SHALL match the old `sandbox.is_command_allowed("rm -rf /") == false` semantics (renamed + inverted)

#### Scenario: Method doc warns about limitations
- **WHEN** `CommandBlacklist` is documented
- **THEN** the struct-level doc comment SHALL contain the literal string "NOT an OS-level sandbox"
- **THEN** the doc comment SHALL mention at least 2 specific bypass techniques (e.g., "unicode obfuscation", "base64 encoding")

---

### Requirement: Blacklist patterns shall be a stable contract

The list of blacklisted patterns SHALL be exported as `pub const BLACKLISTED_PATTERNS: &[&str] = &[...]` for at least 25 patterns (matching the old `sandbox.rs` baseline).

Adding a new pattern SHALL be a non-breaking change. Removing a pattern SHALL be a breaking change requiring a major version bump.

#### Scenario: Pattern list is public
- **WHEN** downstream code references `CommandBlacklist::BLACKLISTED_PATTERNS`
- **THEN** the constant SHALL be `pub`
- **THEN** the constant SHALL contain at least 25 entries

#### Scenario: Pattern list matches old sandbox
- **WHEN** `BLACKLISTED_PATTERNS` is compared element-by-element with the old `sandbox.rs` patterns
- **THEN** all 25 baseline patterns SHALL be present
- **THEN** no pattern SHALL be removed (additions are allowed, removals are breaking)
