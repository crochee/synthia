//! One run's writer over the session sink, plus the helpers that
//! shape prompt text for persistence.

use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use synthia::{
    core::Clock,
    provider::{ContentPart, Role},
    session::{SessionError, SessionSink, SurfaceLedger},
};

/// One run's writer over the session sink.
///
/// Every row the run persists goes through here so the run's
/// [`SurfaceLedger`] sees the exact bytes, in order, with the 1-based
/// log ordinal the reader will reproduce — the source of truth a
/// durable compaction checkpoint resolves its provenance against.
/// Every ordinal this writer records is the one the **sink**
/// returned from the append that produced the row, never a locally
/// counted value — see `append` for why a local counter drifts.
pub(crate) struct RunLog {
    pub(super) store: Arc<dyn SessionSink>,
    pub(super) ledger: Arc<SurfaceLedger>,
    pub(super) next_seq: u64,
    /// Stamps each row's `ts`; the deployment's clock, not a fresh
    /// wall-clock read per append.
    pub(super) clock: synthia::core::SharedClock,
}

impl RunLog {
    /// Wrap the run's sink, seeded with the ordinal of the last row
    /// already on disk.
    pub(crate) fn new(
        store: Arc<dyn SessionSink>,
        ledger: Arc<SurfaceLedger>,
        last_seq: u64,
        clock: synthia::core::SharedClock,
    ) -> Self {
        Self {
            store,
            ledger,
            next_seq: last_seq,
            clock,
        }
    }

    /// Append one row and record it in the ledger under the ordinal
    /// the **sink** assigned.
    ///
    /// The sink is the only thing that can report that ordinal: it
    /// reserves it under its own lock. A counter kept here drifts the
    /// moment anything else appends concurrently — the controller's op
    /// loop writes `Feedback` rows and the shutdown marker straight to
    /// the sink — and the ledger's ordinals are exactly what a
    /// compaction checkpoint's `source_event_seqs` are validated
    /// against, so a drifted one makes a good splice look
    /// unresolvable: `try_fold_log_surface` errors and the lenient
    /// fold silently replays the span the checkpoint had replaced.
    pub(crate) async fn append(
        &mut self,
        value: &Value,
    ) -> Result<u64, SessionError> {
        let seq = self.store.append(value).await?;
        self.next_seq = seq;
        self.ledger.record(seq, value);
        Ok(seq)
    }

    /// Append a typed record, stamping the wall clock and a `seq`.
    ///
    /// The stamp is advisory: the fold that reads these rows assigns
    /// ordinals positionally (`fold_lenient` uses the row index), and
    /// the ledger — what a checkpoint validates against — takes the
    /// sink's authoritative return from `append` above. The stamp can
    /// only be off when another task appended between this run's two
    /// appends, which costs a slightly wrong display value, never a
    /// wrong provenance.
    pub(crate) async fn append_typed(
        &mut self,
        mut value: Value,
    ) -> Result<(), SessionError> {
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "ts".to_string(),
                Value::String(self.clock.now().to_rfc3339()),
            );
            obj.insert("seq".to_string(), Value::from(self.next_seq + 1));
        }
        // `append` returns the sink's per-row stream_index; the
        // typed variant discards it because `seq` is what the
        // ledger tracks. The compiler insists on consuming the
        // `Result<u64, _>` though, so we drop the value
        // explicitly.
        let _idx = self.append(&value).await?;
        Ok(())
    }
}

/// Concatenate the textual component of a multimodal prompt.
///
/// `PromptMulti` carries the user's typed text as a `Text` part
/// alongside any image/audio parts, so the text the durability
/// layer records is not on the run's `prompt` variable. Only text
/// parts contribute — the binary bytes are deliberately not
/// persisted (see the persistence block in the run task).
pub(super) fn prompt_text_of_parts(parts: &[ContentPart]) -> String {
    let mut out = String::new();
    for part in parts {
        if let ContentPart::Text(t) = part {
            out.push_str(&t.text);
        }
    }
    out
}

/// Small, byte-free references to a multimodal prompt's attachments,
/// for the durable `UserInput` row.
///
/// The bytes themselves are deliberately never persisted (see the
/// persistence block in the run task): base64 images would bloat the
/// per-session log and nothing reads them back as bytes. But a reloaded
/// transcript still has to be able to show that the turn carried
/// something — otherwise a file or image turn renders as an empty
/// bubble on the session-detail page while the live chat showed a chip
/// for the very same turn.
///
/// `kind` mirrors the composer's own vocabulary (`"image"` / `"file"`),
/// so a client renders the same chip either way. `name` is omitted when
/// the part carries none rather than being invented here — the renderer
/// owns the fallback label, because only it knows what reads well.
pub(super) fn attachment_refs_of_parts(parts: &[ContentPart]) -> Vec<Value> {
    let mut refs = Vec::new();
    for part in parts {
        let (kind, name, mime_type, byte_len) = match part {
            ContentPart::Image(image) => (
                "image",
                None,
                Some(image.mime_type.clone()),
                // `data` holds either inline base64 or a remote URL (the
                // image counterpart of the resource arm below), so the
                // size is only knowable for the inline form. Deriving one
                // from a URL's length would print a real-looking byte
                // count that is simply wrong.
                remote_url_byte_len(&image.data),
            ),
            ContentPart::Resource(link) => (
                "file",
                Some(link.name.clone()),
                link.mime_type.clone(),
                remote_url_byte_len(&link.uri),
            ),
            // Audio and everything else has no composer-side kind; a
            // text part is the prompt itself, already in `text`.
            _ => continue,
        };
        let mut entry = serde_json::Map::new();
        entry.insert("kind".to_string(), Value::from(kind));
        if let Some(name) = name.filter(|n| !n.is_empty()) {
            entry.insert("name".to_string(), Value::from(name));
        }
        if let Some(mime_type) = mime_type.filter(|m| !m.is_empty()) {
            entry.insert("mime_type".to_string(), Value::from(mime_type));
        }
        if let Some(byte_len) = byte_len {
            entry.insert("byte_len".to_string(), Value::from(byte_len));
        }
        refs.push(Value::Object(entry));
    }
    refs
}

/// The decoded byte length of a media part's `data` field, or `None` when
/// it is not inline bytes.
///
/// Both media arms of [`attachment_refs_of_parts`] accept either an
/// inline payload (bare base64 or a `data:` URI) or a remote URL, so the
/// decision lives here once: a URL has no decodable size, and reporting
/// `len / 4 * 3` of the URL text would render a confident, wrong byte
/// count in the UI.
fn remote_url_byte_len(data: &str) -> Option<u64> {
    let inline = if data.starts_with("data:") {
        true
    } else {
        // A scheme means a reference; anything else is the bare base64
        // the provider parts carry.
        !(data.starts_with("http://") || data.starts_with("https://"))
    };
    inline.then(|| decoded_len_of_base64(data))
}

/// Decoded byte length of a base64 string, without decoding it.
///
/// Tolerates (and skips) any `data:` envelope, so it accepts either the
/// bare payload a provider part carries or the URI form a resource does.
/// Every 4 base64 characters encode 3 bytes, less one byte per `=`
/// padding character, so this is exact for well-formed input.
fn decoded_len_of_base64(data: &str) -> u64 {
    let payload = data
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(',').map(|(_, p)| p))
        .unwrap_or(data);
    let padding = payload.len() - payload.trim_end_matches('=').len();
    let full_groups = (payload.len() / 4) as u64;
    full_groups.saturating_mul(3).saturating_sub(padding as u64)
}

/// The `SurfaceOp::Replace` a rerun's prompt row carries.
///
/// `regenerate` re-queues the last user turn, so a plain append
/// would leave the durable log holding the turn twice
/// (`prompt, answer1, prompt, answer2`) and every log-verbatim
/// projection — the session-detail page, "Continue chat" — would
/// show the prompt twice while the chat page collapses it. The
/// rerun's prompt row instead shadows the turn it replaces: the
/// span from the last user-role message to the tail of the
/// current fold, cited by the shadowed rows' seqs. Every
/// `fold_log_surface` consumer (the next run's history via
/// `events_to_messages`, session search, the regenerate route's
/// own prompt recovery) then folds the log to the replacement
/// story, and the `SurfaceLedger` splices the same span so a
/// later compaction checkpoint still resolves.
///
/// `None` (plain append) when the fold holds no user message —
/// the regenerate route refuses that case up front, so this is a
/// defensive fallback, not a second contract.
pub(super) fn rerun_replace_op(
    events: &[serde_json::Value],
) -> Option<synthia::session::SurfaceOp> {
    let folded = synthia::session::fold_log_surface(events);
    let start = folded.messages.iter().rposition(|row| {
        serde_json::from_value::<synthia::provider::Message>(row.clone())
            .is_ok_and(|message| message.role == Role::User)
    })?;
    Some(synthia::session::SurfaceOp::Replace {
        start,
        end: folded.messages.len(),
        source_event_seqs: folded.surface_seqs[start..].to_vec(),
    })
}

/// Truncate a string to at most `max_chars` Unicode scalar values,
/// appending an ellipsis marker when truncation actually happened.
/// Used only for log previews so a 100kB user prompt does not
/// produce a 100kB log line.
///
/// The truncation is a thin wrapper over
/// [`synthia::core::text::truncate_chars`] so the workspace has a
/// single canonical char-based truncator. The presentation
/// concern (the `…` marker) stays here because the controller's
/// log previews are the only call site that uses this exact
/// shape; other sites that need a different marker (or no
/// marker) call the core helper directly.
pub(super) fn truncate(s: &str, max_chars: usize) -> String {
    let (kept, was_truncated) =
        synthia::core::text::truncate_chars(s, max_chars);
    if was_truncated {
        format!("{kept}…")
    } else {
        kept
    }
}

#[cfg(test)]
mod tests {
    use synthia::provider::{
        ImageContent,
        ResourceLink,
        types::{AudioContent, AudioFormat, TextContent},
    };

    use super::*;

    fn text(body: &str) -> ContentPart {
        ContentPart::Text(TextContent {
            text: body.to_string(),
            cache_control: None,
        })
    }

    /// The prompt's own text is not an attachment, and audio has no
    /// composer-side kind to render — only image/resource parts produce a
    /// reference, so a text or audio part cannot invent a chip for a turn
    /// that showed none.
    #[test]
    fn only_image_and_resource_parts_become_attachment_refs() {
        let parts = vec![
            text("look at this"),
            ContentPart::Audio(AudioContent {
                data: "QUJD".to_string(),
                mime_type: "audio/wav".to_string(),
                format: Some(AudioFormat::Wav),
            }),
        ];
        assert!(attachment_refs_of_parts(&parts).is_empty());
    }

    /// An image reference carries the composer's `image` kind and the mime
    /// type, and reports the DECODED size rather than the base64 length —
    /// a chip that claims 100 bytes for a 75-byte image would be a lie.
    #[test]
    fn image_ref_reports_the_decoded_byte_length() {
        // "AAAA" is 3 bytes; with "==" padding it decodes to 1.
        let parts = vec![ContentPart::Image(ImageContent {
            data: "AAAAAA==".to_string(),
            mime_type: "image/png".to_string(),
            detail: None,
        })];
        let refs = attachment_refs_of_parts(&parts);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["kind"], "image");
        assert_eq!(refs[0]["mime_type"], "image/png");
        assert_eq!(refs[0]["byte_len"], 4, "6 chars = 4 bytes after padding");
        assert!(
            refs[0].get("name").is_none(),
            "an image part carries no filename; the renderer owns the label"
        );
    }

    /// A resource reference keeps the filename the wire gave it, and a
    /// `data:` URI still reports its decoded payload size.
    #[test]
    fn resource_ref_keeps_the_name_and_decodes_a_data_uri() {
        let parts = vec![ContentPart::Resource(ResourceLink {
            uri: "data:application/pdf;base64,QUJD".to_string(),
            name: "probe.pdf".to_string(),
            title: None,
            description: None,
            mime_type: Some("application/pdf".to_string()),
        })];
        let refs = attachment_refs_of_parts(&parts);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["kind"], "file");
        assert_eq!(refs[0]["name"], "probe.pdf");
        assert_eq!(refs[0]["mime_type"], "application/pdf");
        assert_eq!(refs[0]["byte_len"], 3);
    }

    /// A remote-URL resource has no decoded size to report, so the field
    /// is omitted rather than guessed — and an empty name is dropped
    /// rather than persisted as `""` for every renderer to special-case.
    #[test]
    fn a_url_resource_omits_the_size_and_an_empty_name() {
        let parts = vec![ContentPart::Resource(ResourceLink {
            uri: "https://example.com/a.pdf".to_string(),
            name: String::new(),
            title: None,
            description: None,
            mime_type: None,
        })];
        let refs = attachment_refs_of_parts(&parts);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["kind"], "file");
        assert!(refs[0].get("byte_len").is_none());
        assert!(refs[0].get("name").is_none());
        assert!(refs[0].get("mime_type").is_none());
    }
    /// An image part's `data` is inline base64 OR a remote URL — the same
    /// dual shape the resource arm has. A URL has no decodable size, so
    /// the ref must omit `byte_len` rather than report `len / 4 * 3` of
    /// the URL text, which the chip would render as a confident, wrong
    /// size ("18 B" for a 25-char URL).
    #[test]
    fn a_remote_image_url_reports_no_byte_length() {
        let parts = vec![ContentPart::Image(ImageContent {
            data: "https://example.com/a.png".to_string(),
            mime_type: "image/png".to_string(),
            detail: None,
        })];
        let refs = attachment_refs_of_parts(&parts);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["kind"], "image");
        assert_eq!(refs[0]["mime_type"], "image/png");
        assert!(
            refs[0].get("byte_len").is_none(),
            "a URL is not base64 and has no decoded size: {}",
            refs[0]
        );
    }

    /// A `data:`-wrapped payload still reports its decoded size, so the
    /// URL guard above does not swallow the inline case.
    #[test]
    fn a_data_uri_image_still_reports_its_size() {
        let parts = vec![ContentPart::Image(ImageContent {
            data: "data:image/png;base64,QUJD".to_string(),
            mime_type: "image/png".to_string(),
            detail: None,
        })];
        let refs = attachment_refs_of_parts(&parts);
        assert_eq!(refs[0]["byte_len"], 3);
    }
}
