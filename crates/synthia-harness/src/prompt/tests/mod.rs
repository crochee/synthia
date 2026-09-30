//! Test suite for the prompt assembler.
//!
//! Split into focused submodules by concern, so the public
//! builder in the parent module stays a single, small file.
//! Each submodule tests exactly one of the renderers (or the
//! runtime-context snapshot seam), and the cross-cutting
//! fixtures live in [`support`].
//!
//! | Submodule          | What it covers                                            |
//! |--------------------|-----------------------------------------------------------|
//! | [`assemble`]       | The public `assemble` contract — order, purity, byte-stability, no-runtime-facts |
//! | [`identity`]       | The `<identity>` renderer: display_name fallback, empty fields |
//! | [`skills_agents`]  | The `<available_skills>` / `<available_agents>` renderers and their absence rules |
//! | [`runtime_context`]| The `RuntimeContext` snapshot seam: supersession frame, git status, clock injection |
//! | [`support`]        | Shared fixtures (`descriptor_with_instructions`, `peer_descriptor`, `fixed_runtime_context`) |

mod assemble;
mod identity;
mod runtime_context;
mod skills_agents;
mod support;
