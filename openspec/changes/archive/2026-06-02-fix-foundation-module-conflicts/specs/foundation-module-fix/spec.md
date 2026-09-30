## ADDED Requirements

### Requirement: synthia-context Dependencies
The synthia-context crate SHALL have all required dependencies declared in its Cargo.toml including `anyhow`, `sys-locale`, and `regex`.

### Requirement: Module Declaration
The synthia-context crate SHALL declare `prompt_layer` as a module in lib.rs to allow access to `PromptLayer`.

### Requirement: Proper Export of PromptLayer
The `PromptLayer` type SHALL be exported via `pub use prompt_layer::PromptLayer` in lib.rs.

### Requirement: No File Name Conflicts
The crate SHALL NOT have both a standalone `.rs` file and a `./mod.rs` directory for the same module name.

#### Scenario: Build synthia-context
- **WHEN** `cargo build -p synthia-context` is run
- **THEN** it SHALL complete without errors

#### Scenario: PromptLayer import
- **WHEN** external code imports `synthia_context::PromptLayer`
- **THEN** it SHALL resolve correctly

---

## REMOVED Requirements

### Requirement: Standalone prompt.rs

**Reason**: Having both `prompt.rs` (file) and `prompt/` (directory with `mod.rs`) causes E0761 conflict. The standalone file is removed in favor of the module directory.

**Migration**: Update any imports from `synthia_context::prompt::PromptLayer` to `synthia_context::PromptLayer`.