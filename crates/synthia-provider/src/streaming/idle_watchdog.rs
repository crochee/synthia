//! Per-read idle watchdog for provider SSE byte streams (dsh
//! `idleWatchdog` parity).
//!
//! A stalled provider stream must fail loudly instead of parking the
//! agent forever: [`pump_sse`] bounds EVERY outstanding byte-stream
//! read with an idle budget — any chunk read rearms the timer, so the
//! budget applies to the gap between reads, not to the whole call.
//! Expiry surfaces as [`synthia_core::Error::Timeout`] carrying the
//! [`IDLE_TIMEOUT_MARKER`] phrase, so callers can tell an idle stall
//! apart from a connect timeout via [`is_idle_timeout`].
//!
//! `pump_sse` also owns the SSE framing both adapters share: line
//! buffering, the per-line hand-off, the caller-cancellation drain
//! grace, and the trailing non-newline tail.

#[cfg(any(feature = "anthropic", feature = "openai"))]
use std::sync::Arc;
use std::time::Duration;

#[cfg(any(feature = "anthropic", feature = "openai"))]
use futures::{Stream, StreamExt};
#[cfg(any(feature = "anthropic", feature = "openai"))]
use synthia_core::CancelToken;
use synthia_core::Error;

/// Default bound on a single outstanding stream read (~2 min, the dsh
/// `streamIdleTimeoutMs` magnitude): generous enough for the gap
/// between SSE chunks of a slow thinking model, short enough that a
/// dead connection surfaces as an error within one attention span.
pub const DEFAULT_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Marker phrase distinguishing an idle-stall timeout from a connect
/// or request timeout that happens to reuse `Error::Timeout`.
pub const IDLE_TIMEOUT_MARKER: &str = "stream idle timeout";

/// Construct the idle-stall timeout error for a stream that produced
/// no bytes within `timeout` while a read was outstanding.
#[must_use]
pub fn idle_timeout_error(timeout: Duration) -> Error {
    Error::Timeout {
        message: format!(
            "{IDLE_TIMEOUT_MARKER}: no data received for {} ms while a \
             stream read was outstanding",
            timeout.as_millis()
        ),
    }
}

/// Whether `err` is the idle-stall watchdog timeout — as opposed to a
/// connect/request timeout that also uses `Error::Timeout`.
#[must_use]
pub fn is_idle_timeout(err: &Error) -> bool {
    matches!(
        err,
        Error::Timeout { message } if message.contains(IDLE_TIMEOUT_MARKER)
    )
}

/// Grace period the cancellation path gets to drain the body before
/// aborting the connection (mirrors the pre-R30 behaviour of both
/// adapters).
#[cfg(any(feature = "anthropic", feature = "openai"))]
const CANCEL_GRACE: Duration = Duration::from_secs(5);

/// `tokio::select!` arm that resolves when the (optional) cancellation
/// token fires. Returns `Pending` when no token is supplied, so the
/// select! arm is never taken — keeping the loop purely driven by the
/// byte stream.
#[cfg(any(feature = "anthropic", feature = "openai"))]
async fn wait_cancel(token: Option<Arc<dyn CancelToken>>) {
    if let Some(t) = token {
        t.cancelled().await;
    } else {
        // Park forever. The biased select! above plus the manual
        // is_cancelled() check at the top of the loop means we never
        // actually park here in practice; the future is cancelled when
        // its corresponding select! branch is disabled by the guard.
        std::future::pending::<()>().await;
    }
}
/// `true` when an optional cancel token has already fired. Keeps the
/// top-of-loop check a single-expression predicate so the pump
/// orchestrator stays flat.
#[cfg(any(feature = "anthropic", feature = "openai"))]
fn cancelled(token: Option<&(dyn CancelToken + 'static)>) -> bool {
    token.is_some_and(|t| t.is_cancelled())
}
/// Decode one transport chunk, append it to `buf`, and emit any
/// complete SSE lines. Returns `Err` on transport failure; the
/// caller treats `Ok(None)` from the byte stream as end-of-input.
#[cfg(any(feature = "anthropic", feature = "openai"))]
fn append_chunk<B, E, F>(
    buf: &mut Vec<u8>,
    chunk_result: Result<B, E>,
    on_line: &mut F,
) -> Result<(), Error>
where
    B: AsRef<[u8]>,
    E: std::fmt::Display,
    F: FnMut(&str),
{
    let bytes =
        chunk_result.map_err(|e| Error::stream_http_failure(e.to_string()))?;
    buf.extend_from_slice(bytes.as_ref());
    while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
        let line: Vec<u8> = buf.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&line);
        let line = line.trim_end_matches('\n').trim();
        if !line.is_empty() {
            on_line(line);
        }
    }
    Ok(())
}

/// Emit the final non-newline-terminated tail of `buf`, if any. Used
/// when the upstream closes without a trailing `\n`.
#[cfg(any(feature = "anthropic", feature = "openai"))]
fn emit_tail<F>(buf: &[u8], on_line: &mut F)
where
    F: FnMut(&str),
{
    if buf.is_empty() {
        return;
    }
    let tail = String::from_utf8_lossy(buf).trim().to_string();
    if !tail.is_empty() {
        on_line(&tail);
    }
}

/// Drain the byte stream within [`CANCEL_GRACE`] and shape the abort
/// error so the caller's log line still reports whether the drain
/// completed.
#[cfg(any(feature = "anthropic", feature = "openai"))]
async fn abort_after_grace<S, B, E>(byte_stream: &mut S) -> Error
where
    S: Stream<Item = Result<B, E>> + Unpin,
{
    tracing::info!(
        target: "synthia_provider::stream",
        grace_ms = CANCEL_GRACE.as_millis() as u64,
        "stream cancellation requested; draining body up to grace period"
    );
    let drained = tokio::time::timeout(CANCEL_GRACE, async {
        while let Some(item) = byte_stream.next().await {
            if item.is_err() {
                break;
            }
        }
    })
    .await
    .is_ok();
    Error::stream_aborted(format!(
        "stream aborted by caller (drained={drained})"
    ))
}

/// Drive one SSE byte stream to EOF, handing every complete (and the
/// trailing partial) line to `on_line`.
///
/// Shared by both adapters' `complete_with_stream`:
///
/// - **Idle watchdog** — each outstanding read is bounded by
///   `idle_timeout`; any chunk read rearms it. Expiry fails the pump
///   with [`idle_timeout_error`] (classified
///   [`crate::retry::RetryClass::Transient`]).
/// - **Cancellation** — a pre-cancelled token aborts before the first
///   read; a token firing mid-stream gets a `CANCEL_GRACE` best-effort
///   body drain before returning `Error::StreamAborted`.
/// - **Framing** — bytes are buffered and split on `\n`; each trimmed
///   non-empty line (plus a final non-newline-terminated tail) is
///   passed to `on_line` verbatim, so each adapter keeps its own
///   `data:` prefix conventions.
///
/// Transport failures map to `Error::StreamHttpFailure` (via the item
/// error's `Display`), matching the pre-pump adapters.
#[cfg(any(feature = "anthropic", feature = "openai"))]
pub(crate) async fn pump_sse<S, B, E, F>(
    mut byte_stream: S,
    cancel_token: Option<Arc<dyn CancelToken>>,
    idle_timeout: Duration,
    mut on_line: F,
) -> Result<(), Error>
where
    S: Stream<Item = Result<B, E>> + Unpin,
    B: AsRef<[u8]>,
    E: std::fmt::Display,
    F: FnMut(&str),
{
    let mut buf: Vec<u8> = Vec::new();

    loop {
        if cancelled(cancel_token.as_deref()) {
            return Err(Error::stream_aborted("stream cancelled by caller"));
        }

        let next = tokio::select! {
            biased;
            next = byte_stream.next() => next,
            _ = wait_cancel(cancel_token.clone()), if cancel_token.is_some() =>
                return Err(abort_after_grace(&mut byte_stream).await),
            _ = tokio::time::sleep(idle_timeout) =>
                return Err(idle_timeout_error(idle_timeout)),
        };

        let Some(chunk_result) = next else {
            break;
        };
        append_chunk(&mut buf, chunk_result, &mut on_line)?;
    }

    emit_tail(&buf, &mut on_line);
    Ok(())
}

#[cfg(all(test, any(feature = "anthropic", feature = "openai")))]
mod tests {
    use std::{pin::Pin, time::Duration};

    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::retry::{RetryClass, classify_error};

    /// Stream of `Ok` byte chunks that stalls forever after `chunks`
    /// items, with `interval` between yields. The sender holds the
    /// channel open so the receiver's `recv()` stays pending — the
    /// exact shape of a provider that stops sending mid-body.
    fn dripping_stream(
        chunks: usize,
        interval: Duration,
    ) -> Pin<Box<dyn Stream<Item = Result<Vec<u8>, String>> + Send>> {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, String>>(8);
        tokio::spawn(async move {
            for _ in 0..chunks {
                tokio::time::sleep(interval).await;
                if tx.send(Ok(b"data: drip\n".to_vec())).await.is_err() {
                    break;
                }
            }
            // Hold the sender open: the stream never terminates, it
            // just stops producing bytes.
            tokio::time::sleep(Duration::from_secs(3600)).await;
            drop(tx);
        });
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        }))
    }

    #[test]
    fn default_idle_timeout_is_about_two_minutes() {
        assert_eq!(DEFAULT_STREAM_IDLE_TIMEOUT, Duration::from_secs(120));
    }

    #[tokio::test]
    async fn stalled_stream_fires_idle_timeout() {
        let stream = futures::stream::pending::<Result<Vec<u8>, String>>();
        let err = pump_sse(stream, None, Duration::from_millis(80), |_| {})
            .await
            .unwrap_err();
        assert!(is_idle_timeout(&err), "got: {err}");
        assert!(
            err.to_string().contains("stream idle timeout"),
            "message must carry the marker: {err}"
        );
    }

    #[tokio::test]
    async fn idle_timeout_classifies_as_transient() {
        let err = idle_timeout_error(Duration::from_secs(120));
        assert_eq!(classify_error(&err), RetryClass::Transient);
    }

    #[tokio::test]
    async fn watchdog_fires_on_mid_stream_stall() {
        // Two quick chunks, then the stream stalls forever. The
        // watchdog must fire on the stalled THIRD read even though
        // the first two reads succeeded well within budget.
        let stream = dripping_stream(2, Duration::from_millis(10));
        let err = pump_sse(stream, None, Duration::from_millis(120), |_| {})
            .await
            .unwrap_err();
        assert!(is_idle_timeout(&err), "got: {err}");
    }

    #[tokio::test]
    async fn chunk_reads_rearm_the_watchdog() {
        // Five chunks at 60ms intervals under a 150ms idle budget:
        // total elapsed (~300ms) exceeds the budget, so only the
        // per-read rearm can deliver all five. The final read stalls
        // (sender holds the channel open) and fires.
        let stream = dripping_stream(5, Duration::from_millis(60));
        let mut lines = Vec::new();
        let err = pump_sse(stream, None, Duration::from_millis(150), |line| {
            lines.push(line.to_string())
        })
        .await
        .unwrap_err();
        // All five chunks were delivered before the stall.
        assert_eq!(lines.len(), 5);
        assert!(is_idle_timeout(&err), "got: {err}");
    }

    #[tokio::test]
    async fn clean_eof_yields_lines_and_tail() {
        let chunks: Vec<Result<Vec<u8>, String>> = vec![
            Ok(b"data: one\ndata: two\ndata: par".to_vec()),
            Ok(b"tial\n".to_vec()),
            Ok(b"data: tail-no-newline".to_vec()),
        ];
        let mut lines = Vec::new();
        pump_sse(
            futures::stream::iter(chunks),
            None,
            Duration::from_secs(30),
            |line| lines.push(line.to_string()),
        )
        .await
        .unwrap();
        assert_eq!(
            lines,
            vec![
                "data: one",
                "data: two",
                "data: partial",
                "data: tail-no-newline",
            ]
        );
    }

    #[tokio::test]
    async fn transport_error_maps_to_stream_http_failure() {
        let stream = futures::stream::iter(vec![
            Err("kaboom".to_string()) as Result<Vec<u8>, String>
        ]);
        let err = pump_sse(stream, None, Duration::from_secs(30), |_| {})
            .await
            .unwrap_err();
        assert!(
            matches!(&err, Error::StreamHttpFailure { message }
                if message.contains("kaboom")),
            "got: {err}"
        );
        assert!(!is_idle_timeout(&err));
    }

    #[tokio::test]
    async fn pre_cancelled_token_aborts_before_first_read() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let stream = futures::stream::pending::<Result<Vec<u8>, String>>();
        let err = pump_sse(
            stream,
            Some(Arc::new(cancel)),
            Duration::from_secs(30),
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&err, Error::StreamAborted { message }
                if message.contains("cancelled")),
            "got: {err}"
        );
        assert!(!is_idle_timeout(&err));
    }

    #[test]
    fn is_idle_timeout_rejects_connect_timeouts() {
        let connect = Error::Timeout {
            message: "request connect timeout after 30000 ms".into(),
        };
        assert!(!is_idle_timeout(&connect));
        assert!(is_idle_timeout(&idle_timeout_error(
            DEFAULT_STREAM_IDLE_TIMEOUT
        )));
    }

    #[tokio::test]
    async fn empty_stream_completes_without_idle_fire() {
        let stream =
            futures::stream::iter(Vec::<Result<Vec<u8>, String>>::new());
        let mut seen = 0usize;
        pump_sse(stream, None, Duration::from_millis(50), |_| {
            seen += 1;
        })
        .await
        .unwrap();
        assert_eq!(seen, 0);
    }
}
