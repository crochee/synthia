# agents-md-hierarchical-discovery

Add a dedicated `AgentsMdSection` that walks `workspace_dir`'s ancestors for
`AGENTS.md` files, merges them farthest-to-closest with per-file (20K) and
total (60K) char caps, and emits a session-cached block in the system prompt.
Decouples AGENTS.md injection from `IdentitySection` and adds
`agents_md_enabled` / `agents_md_filenames` config to `AgentConfig`.
