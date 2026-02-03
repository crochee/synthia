//! Interaction tools module
//!
//! This module provides user interaction tools.

mod ask;
mod sender;
mod types;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
pub use types::{
    Question,
    QuestionAnswer,
    QuestionOption,
    QuestionRequest,
    QuestionResponse,
};

use crate::Result;

#[async_trait]
pub trait QuestionSender: Send + Sync {
    async fn send_question(
        &self,
        request: QuestionRequest,
    ) -> Result<QuestionResponse>;
}

pub use ask::AskUserQuestionTool;
pub use sender::QuestionSenderImpl;
