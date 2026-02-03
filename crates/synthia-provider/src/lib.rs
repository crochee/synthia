//! LLM service provider module encapsulating interface calls for different language models

mod error;
mod formats;
mod providers;

use std::pin::Pin;

pub use error::ProviderError;
use futures::Stream;
use tokio_util::sync::CancellationToken;

pub type Result<T, E = ProviderError> = core::result::Result<T, E>;

use rmcp::model::{CreateMessageRequestParams, CreateMessageResult};

/// A message stream yields partial text content but complete tool calls, all within the Message object
pub type MessageStream =
    Pin<Box<dyn Stream<Item = Result<CreateMessageResult>> + Send>>;

/// Model provider trait
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    /// Generate streaming response from model
    async fn stream(
        &self,
        params: CreateMessageRequestParams,
        cancel_token: CancellationToken,
    ) -> Result<MessageStream>;
}

pub use formats::collect_stream;
pub use providers::{AnthropicProvider, OpenAICompatibleProvider};
