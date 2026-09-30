//! Opaque cursor encoding for keyset pagination.
//!
//! The cursor is the base64 (URL-safe, no-pad) encoding of the
//! last resource ID on the current page. Clients treat it as
//! opaque; servers decode it to resume via
//! `WHERE id > last_seen_id` (or the in-memory equivalent).
//!
//! This module is the **canonical home** for the cursor codec.
//! `synthia-server` (the HTTP transport) used to ship its own
//! `encode_cursor` / `decode_cursor` with the same logic; that
//! duplication is gone (R79) — the server's wrappers are now
//! 2-line delegations to [`encode`] and [`decode`], and the
//! `synthia-core` registry's private `decode_registry_cursor` /
//! `encode_registry_cursor` are now the same public functions
//! by a different name.
//!
//! # Contract
//!
//! - [`encode`] is the inverse of [`decode`]: for any UTF-8
//!   string `s`, `decode(encode(s)).unwrap() == s` (modulo the
//!   empty-input case where the round-trip is `"" → ""`).
//! - [`decode`] returns [`Error::InvalidItem`] on either a
//!   non-base64 cursor or a non-UTF-8 payload. The
//!   `decode` round-trip never panics.
//! - The encoding is URL-safe (uses `-` and `_` instead of
//!   `+` and `/`) and unpadded (no `=` trailing bytes), so the
//!   cursor is safe to drop into a URL query string without
//!   further escaping.
//! - The encoding is **stable** across versions: a cursor
//!   produced by an older `synthia-core` round-trips on a
//!   newer one and vice versa. Changing the encoding is a
//!   breaking change to any deployment that persists cursors
//!   (e.g. server-side cache, client-side retry tokens).
//!
//! # Why URL-safe no-pad
//!
//! The cursor is part of the wire surface (every `List<T>`
//! response carries a `next_cursor` field). URL-safe base64
//! is the smallest collision-free alphabet that survives
//! every common transport without re-encoding; padding is
//! redundant (the length is recoverable from the encoded
//! characters alone) and the unpadded form keeps the cursor
//! short.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use crate::error::Error;

/// Encode a resource ID into an opaque base64 cursor.
///
/// ```
/// use synthia_core::cursor::encode;
/// assert_eq!(encode("task_abc"), "dGFza19hYmM");
/// ```
#[must_use]
pub fn encode(id: &str) -> String {
    URL_SAFE_NO_PAD.encode(id.as_bytes())
}

/// Decode an opaque base64 cursor back into a resource ID.
///
/// Returns [`Error`] with the [`Error::InvalidItem`] variant if
/// the input is not valid URL-safe base64 or is not valid UTF-8
/// after decoding. Both failure modes share the variant on
/// purpose: a malformed cursor is a wire-level bad-request
/// regardless of the underlying reason, and the client cannot
/// act on the distinction.
#[allow(clippy::result_large_err)] // P1b: `synthia_core::Error` carries 4 hidden fields; result is intentionally large
pub fn decode(cursor: &str) -> Result<String, Error> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_bytes())
        .map_err(|_| Error::invalid_item("cursor"))?;
    String::from_utf8(bytes).map_err(|_| Error::invalid_item("cursor"))
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    use super::*;

    /// The canonical example from the doc comment is the
    /// contract — pinning the literal bytes here means a
    /// refactor that changed the encoding (say, padded
    /// base64) would fail this test, not silently shift
    /// every cursor emitted by the framework.
    #[test]
    fn encode_matches_spec_example() {
        assert_eq!(encode("task_abc"), "dGFza19hYmM");
    }

    /// `decode(encode(s))` is the round-trip every `Registry`
    /// implementation relies on. Pin it for an ASCII ID first;
    /// unicode and the empty / single-char edges are below.
    #[test]
    fn round_trip_ascii_id() {
        let id = "task_abc";
        let encoded = encode(id);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    /// Unicode IDs (the framework does not restrict IDs to
    /// ASCII) must round-trip cleanly. `task_αβγ` encodes
    /// to multi-byte UTF-8; the URL-safe base64 step must
    /// not corrupt the codepoints.
    #[test]
    fn round_trip_unicode_id() {
        let id = "task_αβγ";
        let encoded = encode(id);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, id);
    }

    /// A non-base64 cursor returns `InvalidItem`. The exact
    /// reason does not matter to the wire surface — both
    /// invalid base64 and invalid UTF-8 are 400s.
    #[test]
    fn decode_rejects_invalid_base64() {
        assert!(decode("not-base64!!!").is_err());
    }

    /// A base64 payload that decodes to non-UTF-8 bytes
    /// also returns `InvalidItem`. `0xff` is not valid UTF-8
    /// (standalone) but is a single valid byte, so it
    /// round-trips through `URL_SAFE_NO_PAD` and fails the
    /// `String::from_utf8` check.
    #[test]
    fn decode_rejects_non_utf8_payload() {
        let bad = URL_SAFE_NO_PAD.encode([0xffu8]);
        assert!(decode(&bad).is_err());
    }

    /// The `empty` → `empty` case is not just round-trip
    /// trivia: a handler that wants to encode the empty ID
    /// (which can happen at the start of a list) must get
    /// back a cursor that decodes to the empty string, not
    /// an error.
    #[test]
    fn round_trip_empty_id() {
        let encoded = encode("");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, "");
    }
}
