//! The resume journal: what a run leaves behind so a later one can
//! skip work it already paid for.
//!
//! ## What resume buys
//!
//! A workflow is a document; the documented loop is "edit the document
//! and re-run it". Without a journal that re-pays every agent from
//! scratch, which for a 40-call audit is the entire cost of the run — to
//! change one line of the last stage. With one, the unchanged prefix
//! comes back from disk and only the edit runs.
//!
//! ## Why a *prefix*, and not a lookup table
//!
//! Each entry is keyed by both its position in the run's plan and a hash
//! of everything that decides what that agent does. A replay walks
//! positions in order and stops reusing at the first entry that does not
//! match — every call from there on runs live. Reusing a later match out
//! of order would be reusing a result produced under different upstream
//! conditions: the same prompt at position 12 of a *different* run is
//! not the same work, because what fed it changed.
//!
//! In this crate positions come from a deterministic pre-order walk of
//! the document ([`WorkflowPlan::calls`](crate::WorkflowPlan::calls)),
//! not from call arrival, so a resume that matches matches exactly.
//!
//! ## Failures are never replayed
//!
//! A failed call is journaled as a failure and never replayed as one.
//! Resuming a run that died at call 5 exists to retry call 5, so the
//! prefix ends there and 5 onwards run live — replaying the failure
//! would make it permanent. A skipped call is journaled the same way,
//! for the same reason.
//!
//! The one failure a resume may reuse is a `best_of` candidate that
//! lost: when another candidate of the same step has a successful entry,
//! the step's winner is already decided, so re-running the candidate
//! that failed cannot change anything. That is checked in
//! [`WorkflowPlan::replay_prefix`](crate::WorkflowPlan::replay_prefix),
//! not here: the journal keeps recording what happened. A selection no
//! candidate won has no such entry, so it is retried whole.
//!
//! ## The decision is recorded, not re-derived
//!
//! A `best_of` step's winner is a *decision*, and a host may make it
//! any way it likes ([`WorkflowHost::select_candidate`]), so it is not
//! something a replay can derive from the entries. The run that decides
//! writes the decision as a fresh line for the winner's position
//! ([`JournalEntry::winner`]); a replay reads it back and reproduces
//! the choice instead of asking the host again, which is what keeps a
//! replayed run free of new effects — and a judge call from being paid
//! for twice. An older journal, or a run killed between its last
//! candidate settling and the decision, carries no decision for that
//! step: the resume falls back to the built-in rule, because there is
//! no decision to reproduce.
//!
//! [`WorkflowHost::select_candidate`]: crate::WorkflowHost::select_candidate
//!
//! ## Torn lines and unwritable disks
//!
//! The file is JSON Lines, appended as each call settles, so a run that
//! is killed mid-flight still leaves everything it had finished. A
//! half-written last line is normal: [`read_journal`] skips what it
//! cannot parse and keeps the rest, because a shorter prefix is still a
//! useful one and refusing to run would cost the whole run. For the same
//! reason an append that fails costs a future resume and nothing else.

use std::{collections::BTreeMap, fs::OpenOptions, io::Write, path::Path};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use synthia_core::Clock;

/// One settled agent call, as replayed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Position in the run's plan — the same counter that names calls
    /// in [`WorkflowRun::calls`](crate::WorkflowRun::calls).
    pub position: usize,
    /// Identity hash of the call; a mismatch ends the replayable prefix.
    pub key: String,
    /// Whether the call settled as a success. A failure ends the
    /// replayable prefix, unless it is a `best_of` candidate that lost a
    /// selection the journal already decided.
    pub ok: bool,
    /// The agent's text, when it produced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Why the call failed, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// When the entry was appended.
    pub recorded_at: DateTime<Utc>,
    /// Whether this call is the candidate its `best_of` step selected.
    ///
    /// Written by the run that made the decision, as a fresh entry for
    /// the winner's position once every candidate has settled — the
    /// last line for a position is the one that counts, so a decision
    /// always supersedes the call record it was made from. It is what
    /// lets a replay reproduce a host's choice instead of asking the
    /// host again, and it is deliberately not carried over when an
    /// entry is re-recorded on a replay: the decision belongs to the
    /// run that made it, and this run writes the one it makes.
    #[serde(default, skip_serializing_if = "is_false")]
    pub winner: bool,
}

/// Keep the flag off the wire when it is not set, so an entry that is
/// not a selection's decision stays exactly the line it was before
/// selections had one.
fn is_false(value: &bool) -> bool {
    !*value
}

impl JournalEntry {
    /// A successful call, stamped with the wall clock (through
    /// [`synthia_core::Clock`], not
    /// `chrono::Utc::now`).
    pub fn success(
        position: usize,
        key: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self::success_at(
            position,
            key,
            text,
            synthia_core::SharedClock::system().now(),
        )
    }

    /// [`JournalEntry::success`] with an explicit timestamp — for a
    /// runtime that keeps its own clock.
    pub fn success_at(
        position: usize,
        key: impl Into<String>,
        text: impl Into<String>,
        recorded_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            position,
            key: key.into(),
            ok: true,
            text: Some(text.into()),
            error: None,
            recorded_at,
            winner: false,
        }
    }

    /// A failed call, stamped with the wall clock. `message` says how
    /// it failed — including `skipped by user` for a call live control
    /// skipped.
    pub fn failure(
        position: usize,
        key: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::failure_at(
            position,
            key,
            message,
            synthia_core::SharedClock::system().now(),
        )
    }

    /// [`JournalEntry::failure`] with an explicit timestamp.
    pub fn failure_at(
        position: usize,
        key: impl Into<String>,
        message: impl Into<String>,
        recorded_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            position,
            key: key.into(),
            ok: false,
            text: None,
            error: Some(message.into()),
            recorded_at,
            winner: false,
        }
    }

    /// The same entry, recorded as its step's selected candidate.
    ///
    /// A selection's decision is written as a *fresh* line for the
    /// winner's position rather than by an entry of its own: the last
    /// line for a position is the one a replay reads, so the decision
    /// always supersedes the call record it was decided from.
    #[must_use]
    pub fn selecting(mut self) -> Self {
        self.winner = true;
        self
    }
}

/// The fields that decide what an agent call does.
///
/// Deliberately not the whole call: the step's display name and its
/// phase label move it around in reporting without changing a token the
/// agent sees, so re-labelling should not throw away an hour of results.
/// What is here changes the work — including `isolation`, which changes
/// the filesystem the agent runs against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalKeyInput<'a> {
    /// Position of the call in the run's plan.
    pub position: usize,
    /// Agent the call runs under.
    pub agent: &'a str,
    /// The prompt, before any pipeline chaining.
    pub prompt: &'a str,
    /// The gate command, when the step declares one.
    pub gate: Option<&'a str>,
    /// Whether the call asks for an isolated worktree.
    pub isolation: bool,
}

/// Stable identity hash of a call's payload. Field order is fixed here,
/// not by the caller.
///
/// The prompt is the *document's* prompt: pipeline chaining is derived
/// from the positions before this one, which the prefix walk has already
/// checked, so a matching prefix reproduces the same effective prompt.
pub fn journal_key(input: &JournalKeyInput<'_>) -> String {
    let canonical = serde_json::Value::Array(vec![
        serde_json::Value::from(input.position),
        serde_json::Value::from(input.agent),
        serde_json::Value::from(input.prompt),
        match input.gate {
            Some(gate) => serde_json::Value::from(gate),
            None => serde_json::Value::Null,
        },
        serde_json::Value::from(input.isolation),
    ]);
    // `to_string` on a value built out of strings and numbers cannot
    // fail; the empty string keeps this infallible rather than pushing a
    // dead `Result` into every caller.
    let canonical = canonical.to_string();
    let digest = Sha256::digest(canonical.as_bytes());
    hex::encode(digest)
}

/// Read a journal into position order.
///
/// Never fails: a missing, truncated or hand-mangled journal means
/// "nothing to replay", which costs tokens, while refusing to run would
/// cost the whole run. A partial last line is normal — the file is
/// appended to while calls settle — and is skipped along with any other
/// line that does not parse. Positions are deduplicated (a later line
/// wins, which is what a resume-of-a-resume re-recording looks like) and
/// sorted ascending.
pub fn read_journal(path: &Path) -> Vec<JournalEntry> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut by_position: BTreeMap<usize, JournalEntry> = BTreeMap::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<JournalEntry>(line) {
            Ok(entry) => {
                by_position.insert(entry.position, entry);
            }
            // A half-written final line, or someone editing the file.
            Err(_) => continue,
        }
    }
    by_position.into_values().collect()
}

/// Append one settled call. Failure to write is not failure to run.
///
/// # Errors
///
/// The underlying I/O error, for the caller to log and forget.
pub fn append_journal(
    path: &Path,
    entry: &JournalEntry,
) -> std::io::Result<()> {
    let line = serde_json::to_string(entry)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")
}

/// Entries keyed by position, later lines winning.
///
/// [`read_journal`] already returns them this way; the map is what a
/// prefix walk and the runtime both want to look calls up in.
pub(crate) fn replay_lookup(
    entries: &[JournalEntry],
) -> BTreeMap<usize, &JournalEntry> {
    let mut by_position = BTreeMap::new();
    for entry in entries {
        by_position.insert(entry.position, entry);
    }
    by_position
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_input() -> JournalKeyInput<'static> {
        JournalKeyInput {
            position: 3,
            agent: "scout",
            prompt: "read the tree",
            gate: Some("cargo test"),
            isolation: false,
        }
    }

    #[test]
    fn key_is_stable_and_sensitive_to_its_fields() {
        let baseline = journal_key(&key_input());
        assert_eq!(baseline, journal_key(&key_input()));
        assert_eq!(baseline.len(), 64);

        let moved = JournalKeyInput {
            position: 4,
            ..key_input()
        };
        assert_ne!(baseline, journal_key(&moved));

        let other_gate = JournalKeyInput {
            gate: None,
            ..key_input()
        };
        assert_ne!(baseline, journal_key(&other_gate));

        let isolated = JournalKeyInput {
            isolation: true,
            ..key_input()
        };
        assert_ne!(baseline, journal_key(&isolated));

        let other_prompt = JournalKeyInput {
            prompt: "read the sky",
            ..key_input()
        };
        assert_ne!(baseline, journal_key(&other_prompt));
    }

    #[test]
    fn a_torn_line_costs_that_line_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        let first = JournalEntry::success(0, "a", "one");
        let second = JournalEntry::success(1, "b", "two");
        append_journal(&path, &first).unwrap();
        append_journal(&path, &second).unwrap();

        let mut raw = std::fs::read_to_string(&path).unwrap();
        raw.push_str("{\"position\":2,\"key\":\"c\",\"ok\":tr");
        std::fs::write(&path, raw).unwrap();

        let entries = read_journal(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text.as_deref(), Some("one"));
        assert_eq!(entries[1].text.as_deref(), Some("two"));
    }

    #[test]
    fn unordered_and_duplicated_lines_read_as_one_entry_per_position() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.jsonl");
        let lines = [
            JournalEntry::success(2, "c", "three"),
            JournalEntry::failure(0, "a", "boom"),
            JournalEntry::success(0, "a", "one"),
        ];
        for line in &lines {
            append_journal(&path, line).unwrap();
        }
        // A journal that is not newline-terminated is normal after a kill.
        let raw = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, raw.trim_end()).unwrap();

        let entries = read_journal(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].position, 0);
        assert_eq!(entries[0].text.as_deref(), Some("one"));
        assert!(entries[0].ok);
        assert_eq!(entries[1].position, 2);
        assert_eq!(entries[1].text.as_deref(), Some("three"));
    }

    #[test]
    fn a_missing_journal_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_journal(&dir.path().join("nope.jsonl")).is_empty());
    }
}
