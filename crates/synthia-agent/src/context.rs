use std::sync::Arc;

use async_trait::async_trait;
use synthia_context::{
    assembler::ContextAssembler,
    traits::MessageReader,
    types::ContextError,
};
use synthia_provider::types::{
    CompletionRequest,
    Message,
    ToolChoice,
    ToolDefinition,
};

use crate::config::AgentConfig;

pub struct VecMessageReader {
    messages: Vec<Message>,
}

impl VecMessageReader {
    pub fn new(messages: Vec<Message>) -> Self {
        Self { messages }
    }
}

#[async_trait]
impl MessageReader for VecMessageReader {
    async fn read_messages(&self) -> Result<Vec<Message>, ContextError> {
        Ok(self.messages.clone())
    }

    async fn read_messages_range(
        &self,
        from_index: usize,
        to_index: usize,
    ) -> Result<Vec<Message>, ContextError> {
        let end = to_index.min(self.messages.len());
        if from_index >= end {
            return Ok(vec![]);
        }
        Ok(self.messages[from_index..end].to_vec())
    }

    async fn message_count(&self) -> Result<usize, ContextError> {
        Ok(self.messages.len())
    }
}

pub struct AssembledContext {
    pub request: CompletionRequest,
    pub system_prompt: String,
    pub token_count: usize,
}

pub async fn assemble_context(
    assembler: &ContextAssembler,
    messages: &[Message],
    tool_definitions: Vec<ToolDefinition>,
    config: &AgentConfig,
) -> Result<AssembledContext, ContextError> {
    let reader = VecMessageReader::new(messages.to_vec());
    let mut request = assembler.prepare(&reader).await?;

    request.model = config.model.clone();
    request.tools = Arc::new(tool_definitions);
    request.tool_choice = ToolChoice::Auto;
    request.temperature = config.temperature;
    request.max_tokens = Some(config.max_tokens);
    request.stop_sequences = vec![];
    request.extra_body = None;

    let system_prompt = assembler.build_system_prompt();
    if !system_prompt.is_empty() {
        Arc::make_mut(&mut request.messages)
            .insert(0, Message::system(system_prompt.clone()));
    }

    let token_count =
        synthia_provider::estimate_messages_token_count(&request.messages);

    Ok(AssembledContext {
        request,
        system_prompt,
        token_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_vec_message_reader() {
        let reader = VecMessageReader::new(vec![
            Message::user("hello"),
            Message::assistant("hi"),
        ]);
        let msgs = reader.read_messages().await.unwrap();
        assert_eq!(msgs.len(), 2);
        let count = reader.message_count().await.unwrap();
        assert_eq!(count, 2);
        let range = reader.read_messages_range(0, 1).await.unwrap();
        assert_eq!(range.len(), 1);
    }

    #[test]
    fn test_estimate_token_count() {
        let msgs = vec![Message::user("hello")];
        let count = synthia_provider::estimate_messages_token_count(&msgs);
        assert!(count > 0);
    }
}
