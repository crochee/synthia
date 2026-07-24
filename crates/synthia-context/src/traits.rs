use synthia_provider::Message;

use crate::types::ContextError;

#[async_trait::async_trait]
pub trait MessageReader: Send + Sync {
    async fn read_messages(&self) -> Result<Vec<Message>, ContextError>;

    async fn read_messages_range(
        &self,
        from_index: usize,
        to_index: usize,
    ) -> Result<Vec<Message>, ContextError>;

    async fn message_count(&self) -> Result<usize, ContextError>;
}

/// Extract text content from a message for token estimation.
pub fn extract_message_text(msg: &Message) -> String {
    msg.content.extract_text().unwrap_or_default()
}

/// Extract tool uses from a message.
pub fn extract_message_tool_uses(
    msg: &Message,
) -> Vec<synthia_provider::ToolUse> {
    msg.content.extract_tool_uses()
}

/// Rough token estimation for a message using ~4 chars per token heuristic.
pub fn estimate_message_tokens(message: &Message) -> usize {
    let content_text = extract_message_text(message);
    let content_len = content_text.chars().count();

    let tool_calls_len: usize = extract_message_tool_uses(message)
        .iter()
        .map(|tu| {
            let name_len = tu.name.len();
            let args_len = tu.input.to_string().len();
            tu.id.len() + name_len + args_len
        })
        .sum();

    // Base overhead per message (~4 tokens for role, etc.)
    4 + (content_len + tool_calls_len).div_ceil(4)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use super::*;

    struct TestReader {
        messages: Arc<RwLock<Vec<Message>>>,
    }

    #[async_trait::async_trait]
    impl MessageReader for TestReader {
        async fn read_messages(&self) -> Result<Vec<Message>, ContextError> {
            Ok(self.messages.read().await.clone())
        }

        async fn read_messages_range(
            &self,
            from: usize,
            to: usize,
        ) -> Result<Vec<Message>, ContextError> {
            let msgs = self.messages.read().await;
            let end = to.min(msgs.len());
            if from >= end {
                return Ok(vec![]);
            }
            Ok(msgs[from..end].to_vec())
        }

        async fn message_count(&self) -> Result<usize, ContextError> {
            Ok(self.messages.read().await.len())
        }
    }

    #[tokio::test]
    async fn test_message_reader_read_all() {
        let reader = TestReader {
            messages: Arc::new(RwLock::new(vec![
                Message::user("hello"),
                Message::assistant("hi"),
            ])),
        };
        let msgs = reader.read_messages().await.unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[tokio::test]
    async fn test_message_reader_range() {
        let reader = TestReader {
            messages: Arc::new(RwLock::new(vec![
                Message::user("hello"),
                Message::assistant("hi"),
                Message::user("how are you"),
            ])),
        };
        let msgs = reader.read_messages_range(1, 3).await.unwrap();
        assert_eq!(msgs.len(), 2);
    }

    #[tokio::test]
    async fn test_message_reader_count() {
        let reader = TestReader {
            messages: Arc::new(RwLock::new(vec![Message::user("hello")])),
        };
        let count = reader.message_count().await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_message_reader_empty_range() {
        let reader = TestReader {
            messages: Arc::new(RwLock::new(vec![Message::user("hello")])),
        };
        let msgs = reader.read_messages_range(5, 10).await.unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_extract_message_text() {
        let msg = Message::user("hello world");
        assert_eq!(extract_message_text(&msg), "hello world");
    }

    #[test]
    fn test_estimate_message_tokens() {
        let msg = Message::user("Hello world");
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_extract_tool_uses_empty() {
        let msg = Message::assistant("just text");
        let uses = extract_message_tool_uses(&msg);
        assert!(uses.is_empty());
    }
}
