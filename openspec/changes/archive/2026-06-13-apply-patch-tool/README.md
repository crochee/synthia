# apply-patch-tool

Add an Anthropic V4A `apply_patch` builtin tool to `synthia-tool` that parses
V4A format (`*** Begin Patch` ... `*** End Patch` with `Update File:`, `Add
File:`, `Delete File:`, and `Move to:` operations), applies ops sequentially,
and returns structured `AppliedFailure { applied, failed }` on mid-patch
failure (matching `codex` scenario 015 and `opencode`'s "atomic rollback not
supported yet" stance). Move ops are parsed in V4A grammar but disabled at
runtime by default. The 22 codex portable scenarios are ported as the
canonical V4A compatibility test set.
