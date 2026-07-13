//! Background JSONL writer — codex `RolloutRecorder` pattern.
//!
//! `mpsc::Receiver<TreeCmd>` → batch every 50ms → fsync → ack via oneshot.
//!
//! Appends are ack'd on receipt (not on flush) so the caller returns quickly;
//! durability is amortized via 50ms batching + fsync. Use `Flush` for
//! synchronous durability.

use std::{path::PathBuf, time::Duration};

use tokio::{
    fs::OpenOptions,
    io::{AsyncWriteExt, BufWriter},
    sync::{mpsc, oneshot},
    time::Instant,
};

use crate::{
    entry::SessionEntry,
    error::{Result, SessionError},
};

/// Commands sent to the writer task.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TreeCmd {
    Append {
        entry: SessionEntry,
        ack: oneshot::Sender<Result<()>>,
    },
    Flush {
        ack: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        ack: oneshot::Sender<()>,
    },
}

#[allow(clippy::large_enum_variant)]
enum PendingCmd {
    Entry(SessionEntry),
    FlushAck(oneshot::Sender<Result<()>>),
}

/// Background writer task. Spawn via `tokio::spawn`.
pub async fn session_writer_task(
    path: PathBuf,
    mut rx: mpsc::Receiver<TreeCmd>,
) {
    let file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(path = ?path, error = %e, "writer_task: failed to open file");
            let io_err = SessionError::Io(e);
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    TreeCmd::Append { ack, .. } => {
                        let _ = ack.send(Err(SessionError::Io(
                            std::io::Error::other(io_err.to_string()),
                        )));
                    }
                    TreeCmd::Flush { ack } => {
                        let _ = ack.send(Err(SessionError::Io(
                            std::io::Error::other(io_err.to_string()),
                        )));
                    }
                    TreeCmd::Shutdown { ack } => {
                        let _ = ack.send(());
                    }
                }
            }
            return;
        }
    };
    let mut writer = BufWriter::new(file);

    loop {
        let first = match rx.recv().await {
            Some(cmd) => cmd,
            None => break,
        };

        // Ack the first cmd immediately so caller returns; then drain the
        // 50ms batch window for more work.
        let mut batch: Vec<PendingCmd> = Vec::with_capacity(16);
        match first {
            TreeCmd::Append { entry, ack } => {
                let _ = ack.send(Ok(()));
                batch.push(PendingCmd::Entry(entry));
            }
            TreeCmd::Flush { ack } => {
                batch.push(PendingCmd::FlushAck(ack));
            }
            TreeCmd::Shutdown { ack } => {
                let _ = writer.flush().await;
                let _ = ack.send(());
                return;
            }
        }

        let deadline = Instant::now() + Duration::from_millis(50);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(cmd)) => match cmd {
                    TreeCmd::Append { entry, ack } => {
                        let _ = ack.send(Ok(()));
                        batch.push(PendingCmd::Entry(entry));
                    }
                    TreeCmd::Flush { ack } => {
                        batch.push(PendingCmd::FlushAck(ack));
                    }
                    TreeCmd::Shutdown { ack } => {
                        for item in batch.drain(..) {
                            match item {
                                PendingCmd::Entry(e) => {
                                    let _ = append_entry(&mut writer, &e).await;
                                }
                                PendingCmd::FlushAck(a) => {
                                    let _ = a.send(Ok(()));
                                }
                            }
                        }
                        let _ = writer.flush().await;
                        let _ = ack.send(());
                        return;
                    }
                },
                Ok(None) => break,
                Err(_) => break,
            }
        }

        // Write all batched entries + flush.
        for item in batch.drain(..) {
            match item {
                PendingCmd::Entry(e) => {
                    let _ = append_entry(&mut writer, &e).await;
                }
                PendingCmd::FlushAck(a) => {
                    let _ = a.send(Ok(()));
                }
            }
        }
        let _ = writer.flush().await;
    }
}

async fn append_entry<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    entry: &SessionEntry,
) -> Result<()> {
    let line = serde_json::to_string(entry)?;
    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}
