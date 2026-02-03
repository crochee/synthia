use std::{collections::HashMap, fmt, sync::Arc, time::Duration};

use tokio::{
    sync::{Mutex, RwLock, mpsc, oneshot, watch},
    time::timeout,
};
use tracing::warn;

use super::{
    QuestionSender,
    types::{QuestionRequest, QuestionResponse},
};
use crate::{AgentError, Result};

const TIMEOUT_DURATION: Duration = Duration::from_secs(300);

pub struct QuestionSenderImpl {
    pending: Arc<RwLock<HashMap<String, oneshot::Sender<QuestionResponse>>>>,
    request_tx: mpsc::UnboundedSender<QuestionRequest>,
    request_rx: Mutex<mpsc::UnboundedReceiver<QuestionRequest>>,
    question_watcher: watch::Sender<()>,
}

impl fmt::Debug for QuestionSenderImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuestionSenderImpl").finish()
    }
}

impl Default for QuestionSenderImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl QuestionSenderImpl {
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let (question_watcher, _) = watch::channel(());
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            request_tx,
            request_rx: Mutex::new(request_rx),
            question_watcher,
        }
    }

    pub fn request_rx(
        &self,
    ) -> &Mutex<mpsc::UnboundedReceiver<QuestionRequest>> {
        &self.request_rx
    }

    pub fn question_watch(&self) -> watch::Receiver<()> {
        self.question_watcher.subscribe()
    }

    pub async fn submit_response(
        &self,
        request_id: String,
        response: QuestionResponse,
    ) -> Result<()> {
        let sender = {
            let mut pending = self.pending.write().await;
            pending.remove(&request_id).ok_or_else(|| {
                AgentError::InvalidOperation(format!(
                    "Request not found: {request_id}"
                ))
            })?
        };

        sender.send(response).ok();

        Ok(())
    }
}

#[async_trait::async_trait]
impl QuestionSender for QuestionSenderImpl {
    async fn send_question(
        &self,
        request: QuestionRequest,
    ) -> Result<QuestionResponse> {
        let request_id = request.id.clone();
        let (tx, rx) = oneshot::channel();

        self.pending.write().await.insert(request_id.clone(), tx);

        if let Err(e) = self.request_tx.send(request) {
            warn!("Failed to send ask question message: {}", e);
            return Err(AgentError::InvalidOperation(
                "Failed to send message".to_string(),
            ));
        }

        // Notify about new question
        let _ = self.question_watcher.send(());

        let result = match timeout(TIMEOUT_DURATION, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                warn!("Response channel closed for request: {}", request_id);
                Err(AgentError::InvalidOperation(
                    "Response channel closed".to_string(),
                ))
            }
            Err(_) => {
                warn!("Timeout waiting for response: {}", request_id);
                Err(AgentError::Timeout(
                    "Timeout waiting for user response".to_string(),
                ))
            }
        };

        self.pending.write().await.remove(&request_id);

        result
    }
}
