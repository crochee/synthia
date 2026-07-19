# prefix-source-trait Specification

## Purpose
TBD - created by archiving change system-context-source-epoch. Update Purpose after archive.
## Requirements
### Requirement: Source trait SHALL define baseline/update lifecycle for prefix-affecting content

`Source` SHALL be a trait in `synthia_context::source` (or `synthia_context::prefix_tracker::source`) with three methods:
- `fn id(&self) -> SourceId` — returns a stable, type-safe identifier for this source
- `fn baseline(&self) -> SourceContent` — returns the initial epoch content
- `fn update(&mut self) -> SourceDelta` — returns the delta since the last call (`Changed`, `Unchanged`, or `Removed`)

`Source` SHALL require `Send + Sync`. `Source` SHALL NOT expose mutable access to internal state outside of `update()`.

`SourceId` SHALL be a newtype wrapping `&'static str` (or `Cow<'static, str>`) with `Eq + Hash + Clone + Debug`. Two sources with the same `id()` SHALL be considered the same source.

`SourceContent` SHALL be a newtype wrapping `Vec<u8>` (or `String`) with a `hash(&self) -> u64` method using `ahash::AHasher::default()` (deterministic).

#### Scenario: Source baseline is stable across calls without update

- **WHEN** `source.baseline()` is called twice without an intervening `update()`
- **THEN** both calls SHALL return `SourceContent` with identical `hash()` values
- **AND** the baseline content SHALL NOT be mutated by `baseline()` calls

#### Scenario: Source update returns Unchanged when content identical

- **WHEN** `source.update()` is called and the underlying content has not changed since the last `baseline()` or `update()` call
- **THEN** the returned `SourceDelta` SHALL be `SourceDelta::Unchanged`

#### Scenario: Source update returns Changed when content differs

- **WHEN** `source.update()` is called and the underlying content has changed since the last `baseline()` or `update()` call
- **THEN** the returned `SourceDelta` SHALL be `SourceDelta::Changed(new_content)`
- **AND** `new_content.hash()` SHALL differ from the previously recorded hash

#### Scenario: Source update returns Removed when source no longer contributes

- **WHEN** `source.update()` is called and the source has been deactivated (e.g., a skill was unloaded)
- **THEN** the returned `SourceDelta` SHALL be `SourceDelta::Removed`

---

### Requirement: SourceDelta SHALL encode Changed, Unchanged, and Removed variants

`SourceDelta` SHALL be an enum with three variants:
- `Changed(SourceContent)` — content changed; carries the new content
- `Unchanged` — content identical to last recorded state
- `Removed` — source no longer contributes to the prefix

`SourceDelta` SHALL derive `Clone, Debug`. `Changed` variant SHALL expose the inner `SourceContent` by reference and by value.

#### Scenario: Unchanged variant carries no payload

- **WHEN** `SourceDelta::Unchanged` is constructed
- **THEN** it SHALL be a unit variant with no associated data

#### Scenario: Removed variant carries no payload

- **WHEN** `SourceDelta::Removed` is constructed
- **THEN** it SHALL be a unit variant with no associated data

---

### Requirement: SourceEpoch SHALL track baseline and current hash for diff detection

`SourceEpoch` SHALL be a struct with three fields:
- `baseline_hash: u64` — hash of the content at the start of the epoch
- `current_hash: u64` — hash of the most recently recorded content
- `content: SourceContent` — the most recently recorded content

`SourceEpoch::is_changed(&self) -> bool` SHALL return `true` when `baseline_hash != current_hash` and `false` when they are equal.

`SourceEpoch` SHALL be constructed from a `SourceContent` via `SourceEpoch::new(content)` which sets `baseline_hash` and `current_hash` to the same value (the content's hash).

`SourceEpoch::apply_delta(&mut self, delta: SourceDelta)` SHALL update `current_hash` and `content` when delta is `Changed`, leave them unchanged when `Unchanged`, and mark the epoch as removed (via an internal `removed: bool` flag or equivalent) when `Removed`.

#### Scenario: Newly constructed epoch reports no change

- **WHEN** `SourceEpoch::new(content)` is constructed
- **THEN** `is_changed()` SHALL return `false`
- **AND** `baseline_hash` SHALL equal `current_hash`

#### Scenario: Changed delta flips is_changed to true

- **WHEN** `epoch.apply_delta(SourceDelta::Changed(new_content))` is called with `new_content.hash() != epoch.baseline_hash`
- **THEN** `epoch.is_changed()` SHALL return `true`
- **AND** `epoch.current_hash` SHALL equal `new_content.hash()`

#### Scenario: Unchanged delta preserves current state

- **WHEN** `epoch.apply_delta(SourceDelta::Unchanged)` is called
- **THEN** `epoch.current_hash` SHALL remain unchanged
- **AND** `epoch.is_changed()` SHALL return the same value as before the call

---

### Requirement: CacheBreakDetector SHALL use SourceEpoch HashMap keyed by SourceId

`CacheBreakDetector.state_by_source` SHALL be of type `HashMap<SourceId, SourceEpoch>` (replacing the previous `HashMap<String, TrackedState>`).

`CacheBreakDetector::record_source(&mut self, source: &dyn Source)` SHALL:
1. Look up `source.id()` in the map
2. If absent, insert a new `SourceEpoch::new(source.baseline())`
3. If present, call `source.update()` and `epoch.apply_delta(delta)`

`CacheBreakDetector::check_cache_break(&self) -> CacheBreakReport` SHALL iterate over `state_by_source` and for each entry:
- If `epoch.is_changed()` is `true`, add the `SourceId` to the report's `changed_sources` set
- If the epoch is marked removed, add the `SourceId` to the report's `removed_sources` set

The `CacheBreakReport` SHALL expose:
- `changed_sources: Vec<SourceId>` — sources whose baseline != current
- `removed_sources: Vec<SourceId>` — sources that returned `Removed` delta
- `system_prompt_changed: bool` — derived: `changed_sources` contains the system prompt source id
- `tool_schemas_changed: bool` — derived: `changed_sources` contains the tool schemas source id

`check_cache_break` SHALL NOT use the previous broken `if hash != 0` comparison. The diff SHALL be a strict `baseline_hash != current_hash` equality check.

#### Scenario: Unchanged sources produce empty report

- **WHEN** `check_cache_break` is called after recording 3 sources with no `Changed` deltas
- **THEN** `changed_sources` SHALL be empty
- **AND** `removed_sources` SHALL be empty
- **AND** `system_prompt_changed` SHALL be `false`
- **AND** `tool_schemas_changed` SHALL be `false`

#### Scenario: System prompt change is detected and attributed

- **WHEN** the system prompt source returns `Changed` delta
- **AND** `check_cache_break` is called
- **THEN** `changed_sources` SHALL contain the system prompt source id
- **AND** `system_prompt_changed` SHALL be `true`
- **AND** `tool_schemas_changed` SHALL be `false` (other sources unchanged)

#### Scenario: Removed source is reported separately

- **WHEN** a source returns `Removed` delta
- **AND** `check_cache_break` is called
- **THEN** `removed_sources` SHALL contain that source id
- **AND** `changed_sources` SHALL NOT contain that source id

#### Scenario: Zero hash no longer triggers false positive

- **WHEN** a source's `current_hash` happens to be `0` but equals `baseline_hash`
- **THEN** `is_changed()` SHALL return `false`
- **AND** `check_cache_break` SHALL NOT report that source as changed

---

### Requirement: SystemPromptSource SHALL track system prompt text content

`SystemPromptSource` SHALL implement `Source` with `id()` returning a `SourceId` derived from `"system-prompt"`.

`SystemPromptSource::new(text: String)` SHALL store the text and compute the baseline content.

`baseline()` SHALL return `SourceContent` derived from the stored text.

`update(&mut self, new_text: String)` SHALL compare the new text's hash against the stored hash and return `Changed`, `Unchanged`, accordingly. (Note: the `Source::update` trait method takes `&mut self` and no arguments; `SystemPromptSource` SHALL hold a setter or be constructed fresh per context build with the new text, comparing against the previously recorded baseline.)

For the initial implementation, `SystemPromptSource` SHALL be constructed fresh per `ContextAssembler::assemble` call with the current system prompt text. The `CacheBreakDetector` SHALL hold the previous epoch and compare via `record_source`.

#### Scenario: Identical system prompt text across builds reports Unchanged

- **WHEN** two consecutive context builds use the same system prompt text
- **THEN** the second `record_source` SHALL apply `Unchanged` delta
- **AND** `check_cache_break` SHALL report `system_prompt_changed: false`

#### Scenario: Different system prompt text reports Changed

- **WHEN** the second context build uses a different system prompt text
- **THEN** `record_source` SHALL apply `Changed` delta
- **AND** `check_cache_break` SHALL report `system_prompt_changed: true`

---

### Requirement: ToolSchemasSource SHALL track canonical tool schema JSON

`ToolSchemasSource` SHALL implement `Source` with `id()` returning a `SourceId` derived from `"tool-schemas"`.

`ToolSchemasSource::new(tools: &[ToolDefinition])` SHALL serialize the tools to canonical JSON (sorted keys, deterministic whitespace) and store the resulting bytes as the baseline content.

`baseline()` SHALL return `SourceContent` from the stored canonical JSON bytes.

The canonical serialization SHALL sort tool definitions by name and serialize each definition's JSON with `serde_json` using `to_string_pretty` with sorted keys (or an equivalent deterministic serializer) to ensure byte-level stability across calls.

#### Scenario: Identical tool sets produce identical baseline hash

- **WHEN** `ToolSchemasSource::new` is called twice with the same set of `ToolDefinition`s (in any order)
- **THEN** both instances' `baseline().hash()` SHALL be equal

#### Scenario: Reordered tools produce identical hash

- **WHEN** `ToolSchemasSource::new` is called with tools `[A, B, C]` and then `[B, A, C]`
- **THEN** both instances' `baseline().hash()` SHALL be equal (canonical sort normalizes order)

#### Scenario: Different tool set produces different hash

- **WHEN** `ToolSchemasSource::new` is called with `[A, B]` and then `[A, B, C]`
- **THEN** the two `baseline().hash()` values SHALL differ

---

### Requirement: SkillListSource SHALL track skill identifiers and return Unchanged until SkillProvider delta is wired

`SkillListSource` SHALL implement `Source` with `id()` returning a `SourceId` derived from `"skill-list"`.

`SkillListSource::new(skill_ids: Vec<String>)` SHALL sort the skill_ids, join them with a separator, and store the resulting string as baseline content.

For the initial implementation, `update()` SHALL always return `SourceDelta::Unchanged`. The `SkillProvider` delta detection mechanism is deferred to a separate change; until then `SkillListSource` exists to occupy the Source slot so that future activation requires only swapping `update()` implementation.

#### Scenario: SkillListSource reports Unchanged by default

- **WHEN** `SkillListSource::new(["a", "b"]).update()` is called
- **THEN** the returned delta SHALL be `SourceDelta::Unchanged`

#### Scenario: Different skill sets produce different baseline hash

- **WHEN** `SkillListSource::new(["a", "b"])` and `SkillListSource::new(["a", "c"])` are constructed
- **THEN** their `baseline().hash()` values SHALL differ

