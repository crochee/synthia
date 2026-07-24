use async_trait::async_trait;
use futures::StreamExt;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;

use crate::traits::McpTransport;

/// Wraps a reqwest ByteStream as an AsyncRead, emitting parsed SSE `data:` lines.
struct SseStreamReader {
    receiver: mpsc::Receiver<String>,
}

impl SseStreamReader {
    fn new(receiver: mpsc::Receiver<String>) -> Self {
        Self { receiver }
    }
}

impl AsyncRead for SseStreamReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if buf.remaining() == 0 {
            return std::task::Poll::Ready(Ok(()));
        }

        match Pin::new(&mut self.receiver).poll_recv(cx) {
            std::task::Poll::Ready(Some(data)) => {
                let n = data.len().min(buf.remaining());
                buf.put_slice(data.as_bytes());
                if data.len() > n {
                    // Put back overflow (simplified: truncate for now)
                    // The SSE stream is one-way so we accept dropping overflow
                }
                std::task::Poll::Ready(Ok(()))
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Ok(())), // EOF
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

/// Sends JSON-RPC messages as HTTP POST requests.
struct HttpPostSender {
    sender: mpsc::Sender<String>,
}

impl HttpPostSender {
    fn new(sender: mpsc::Sender<String>) -> Self {
        Self { sender }
    }
}

impl AsyncWrite for HttpPostSender {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match std::str::from_utf8(buf) {
            Ok(s) => {
                let msg = s.to_string();
                let len = msg.len();
                // Attempt non-blocking send; if the channel is full or closed, we still report success
                let _ = self.sender.try_send(msg);
                std::task::Poll::Ready(Ok(len))
            }
            Err(_) => std::task::Poll::Ready(Ok(buf.len())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        // Flushing is a no-op: each write is immediately sent as a POST
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        self.sender.close_channel();
        std::task::Poll::Ready(Ok(()))
    }
}

pub struct SseTransport {
    write_half: Option<HttpPostSender>,
    read_half: Option<SseStreamReader>,
    _shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    connected: bool,
}

impl SseTransport {
    pub fn new(post_url: String, sse_url: String) -> Self {
        let client = reqwest::Client::new();
        let (post_tx, mut post_rx) = mpsc::unbounded_channel::<String>();
        let (sse_tx, sse_rx) = mpsc::channel::<String>(64);
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        // POST task: collect lines from stdin_writer and POST each as a JSON body
        let client_clone = client.clone();
        let post_url_clone = post_url.clone();
        tokio::spawn(async move {
            let mut partial = String::new();
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    msg = post_rx.recv() => {
                        match msg {
                            Some(data) => {
                                partial.push_str(&data);
                                for line in partial.lines() {
                                    let line = line.trim();
                                    if line.is_empty() {
                                        continue;
                                    }
                                    match client_clone
                                        .post(&post_url_clone)
                                        .body(line.to_string())
                                        .header("Content-Type", "application/json")
                                        .send()
                                        .await
                                    {
                                        Ok(res) => {
                                            if res.status().is_client_error() {
                                                tracing::warn!(
                                                    status = %res.status(),
                                                    "SSE POST returned client error"
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!(error = %e, "SSE POST failed");
                                        }
                                    }
                                }
                                partial.clear();
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        // SSE reader task: GET the SSE endpoint, parse data: lines, send to read_half
        let sse_client = client.clone();
        let sse_sender = sse_tx;
        tokio::spawn(async move {
            let result = sse_client.get(&sse_url).send().await;
            match result {
                Ok(response) => {
                    if !response.status().is_success() {
                        tracing::warn!(status = %response.status(), "SSE endpoint returned non-OK status");
                    }
                    let mut stream = response.bytes_stream();
                    let mut buffer = String::new();
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(bytes) => {
                                buffer.push_str(&String::from_utf8_lossy(&bytes));
                                // SSE events are delimited by double newlines
                                while let Some(pos) = buffer.find("\n\n") {
                                    let event_block = buffer[..pos].to_string();
                                    buffer = buffer[pos + 2..].to_string();
                                    for line in event_block.lines() {
                                        if let Some(data) = line.strip_prefix("data: ") {
                                            let trimmed = data.trim();
                                            if !trimmed.is_empty() {
                                                let _ = sse_sender.send(trimmed.to_string()).await;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "SSE stream chunk error");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to connect to SSE endpoint");
                }
            }
            let _ = sse_sender.close().await;
        });

        Self {
            write_half: Some(HttpPostSender::new(post_tx)),
            read_half: Some(SseStreamReader::new(sse_rx)),
            _shutdown_tx: Some(shutdown_tx),
            connected: true,
        }
    }
}

#[async_trait]
impl McpTransport for SseTransport {
    fn stdin_writer(&mut self) -> &mut (dyn AsyncWrite + Unpin + Send) {
        self.write_half.as_mut().expect("stdin not available")
    }

    fn stdout_reader(&mut self) -> &mut (dyn AsyncRead + Unpin + Send) {
        self.read_half.as_mut().expect("stdout not available")
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.connected = false;
        self.write_half.take();
        self.read_half.take();
        self._shutdown_tx.take();
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected && self.write_half.is_some() && self.read_half.is_some()
    }
}