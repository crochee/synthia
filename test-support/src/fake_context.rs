use std::sync::Arc;

use synthia_context::{traits::MessageReader, types::ContextError};
use synthia_provider::Message;
use tokio::sync::RwLock;

/// Fake implementation of MessageReader for testing.
#[derive(Clone)]
pub struct FakeMessageReader {
    messages: Arc<RwLock<Vec<Message>>>,
}

impl FakeMessageReader {
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages: Arc::new(RwLock::new(messages)),
        }
    }

    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = Arc::new(RwLock::new(messages));
        self
    }

    pub async fn push_message(&self, msg: Message) {
        self.messages.write().await.push(msg);
    }

    pub async fn clear(&self) {
        self.messages.write().await.clear();
    }
}

#[async_trait::async_trait]
impl MessageReader for FakeMessageReader {
    async fn read_messages(&self) -> Result<Vec<Message>, ContextError> {
        Ok(self.messages.read().await.clone())
    }

    async fn read_messages_range(
        &self,
        from_index: usize,
        to_index: usize,
    ) -> Result<Vec<Message>, ContextError> {
        let msgs = self.messages.read().await;
        let end = to_index.min(msgs.len());
        if from_index >= end {
            return Ok(vec![]);
        }
        Ok(msgs[from_index..end].to_vec())
    }

    async fn message_count(&self) -> Result<usize, ContextError> {
        Ok(self.messages.read().await.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fake_reader_read_all() {
        let reader = FakeMessageReader::new(vec![
            Message::user("hello"),
            Message::assistant("hi"),
        ]);
        let msgs = reader.read_messages().await.unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[tokio::test]
    async fn test_fake_reader_range() {
        let reader = FakeMessageReader::new(vec![
            Message::user("msg1"),
            Message::assistant("msg2"),
            Message::user("msg3"),
        ]);
        let msgs = reader.read_messages_range(1, 3).await.unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[tokio::test]
    async fn test_fake_reader_count() {
        let reader = FakeMessageReader::new(vec![Message::user("hello")]);
        assert_eq!(reader.message_count().await.unwrap(), 1);
    }
}
