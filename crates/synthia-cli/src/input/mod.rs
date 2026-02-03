pub mod media;

use std::{
    io::{self, Write},
    path::Path,
    sync::Arc,
};

pub use media::{MediaAttachment, MediaProcessor, MediaType};
use synthia_agent::tools::{
    Question,
    QuestionAnswer,
    QuestionRequest,
    QuestionResponse,
    QuestionSenderImpl,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::color::colors;

#[derive(Debug, Clone)]
pub enum InputEvent {
    MultimodalInput {
        text: String,
        attachments: Vec<MediaAttachment>,
    },
    Command(InputCommand),
}

#[derive(Debug, Clone)]
pub enum InputCommand {
    Quit,
    Help,
    Clear,
    Compact,
    SessionNew,
    SessionSwitch(String),
    SessionList,
    Reasoning,
    Speed(u64),
    Attach(String),
    Attachments,
    ClearAttachments,
    Export(String),
    Token,
    History(Option<usize>),
}

impl InputCommand {
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if !line.starts_with('/') {
            return None;
        }

        let parts: Vec<&str> = line[1..].splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let arg = parts.get(1).map(|s| s.trim());

        match cmd.as_str() {
            "quit" | "exit" => Some(InputCommand::Quit),
            "help" => Some(InputCommand::Help),
            "clear" => Some(InputCommand::Clear),
            "compact" => Some(InputCommand::Compact),
            "session" => match arg {
                None => Some(InputCommand::SessionNew),
                Some(id) => Some(InputCommand::SessionSwitch(id.to_string())),
            },
            "sessions" => Some(InputCommand::SessionList),
            "reasoning" => Some(InputCommand::Reasoning),
            "speed" => {
                let speed = arg.and_then(|s| s.parse().ok()).unwrap_or(8);
                Some(InputCommand::Speed(speed))
            }
            "attach" => {
                let path = arg?.to_string();
                Some(InputCommand::Attach(path))
            }
            "attachments" => Some(InputCommand::Attachments),
            "clear-attachments" => Some(InputCommand::ClearAttachments),
            "export" => {
                let format = arg.unwrap_or("markdown").to_string();
                Some(InputCommand::Export(format))
            }
            "token" => Some(InputCommand::Token),
            "history" => {
                let count = arg.and_then(|s| s.parse().ok());
                Some(InputCommand::History(count))
            }
            _ => None,
        }
    }

    pub fn parse_answer(
        line: &str,
        question: &Question,
    ) -> Option<QuestionResponse> {
        let line = line.trim();

        if question.multi_select {
            parse_multi_select(line, question)
        } else {
            parse_single_select(line, question)
        }
    }
}

fn parse_multi_select(
    line: &str,
    question: &Question,
) -> Option<QuestionResponse> {
    let selections: Vec<&str> = line.split(',').map(|s| s.trim()).collect();

    let valid_indices: Vec<usize> = selections
        .iter()
        .filter_map(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0 && n <= question.options.len())
        .collect();

    if valid_indices.len() != selections.len() {
        return None;
    }

    let selected: Vec<String> = valid_indices
        .iter()
        .filter_map(|&n| question.options.get(n - 1))
        .map(|o| o.label.clone())
        .collect();

    if selected.is_empty() {
        return None;
    }

    Some(QuestionResponse {
        request_id: String::new(),
        answers: vec![QuestionAnswer {
            selected,
            other: None,
        }],
    })
}

fn parse_single_select(
    line: &str,
    question: &Question,
) -> Option<QuestionResponse> {
    if let Ok(n) = line.parse::<usize>()
        && n > 0
        && n <= question.options.len()
    {
        let opt = &question.options[n - 1];
        return Some(QuestionResponse {
            request_id: String::new(),
            answers: vec![QuestionAnswer {
                selected: vec![opt.label.clone()],
                other: None,
            }],
        });
    }

    if !line.is_empty() {
        return Some(QuestionResponse {
            request_id: String::new(),
            answers: vec![QuestionAnswer {
                selected: Vec::new(),
                other: Some(line.to_string()),
            }],
        });
    }

    None
}

#[derive(Clone)]
pub struct PendingAttachments {
    attachments: Vec<MediaAttachment>,
}

impl PendingAttachments {
    pub fn new() -> Self {
        Self {
            attachments: Vec::new(),
        }
    }

    pub fn add(&mut self, attachment: MediaAttachment) {
        self.attachments.push(attachment);
    }

    pub fn clear(&mut self) {
        self.attachments.clear();
    }

    pub fn take(&mut self) -> Vec<MediaAttachment> {
        std::mem::take(&mut self.attachments)
    }

    pub fn list(&self) -> &[MediaAttachment] {
        &self.attachments
    }

    pub fn is_empty(&self) -> bool {
        self.attachments.is_empty()
    }

    pub fn count(&self) -> usize {
        self.attachments.len()
    }
}

impl Default for PendingAttachments {
    fn default() -> Self {
        Self::new()
    }
}

pub struct InputHandler {
    event_tx: mpsc::UnboundedSender<InputEvent>,
    question_sender: Arc<QuestionSenderImpl>,
    pending_attachments: PendingAttachments,
}

impl InputHandler {
    pub fn new(
        event_tx: mpsc::UnboundedSender<InputEvent>,
        question_sender: Arc<QuestionSenderImpl>,
    ) -> Self {
        Self {
            event_tx,
            question_sender,
            pending_attachments: PendingAttachments::new(),
        }
    }

    #[instrument(skip_all, name = "input_handler_run")]
    pub async fn run(&mut self, cancel_token: CancellationToken) {
        let mut question_watcher = self.question_sender.question_watch();

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    break;
                }
                _ = question_watcher.changed() => {
                    if let Ok(mut guard) = self.question_sender.request_rx().try_lock() {
                        tokio::select! {
                            _ = cancel_token.cancelled() => {
                                break;
                            }
                            req = guard.recv() => {
                                if let Some(req) = req {
                                    self.handle_question(req).await;
                                }
                            }
                        }
                    }
                }
                _ = self.handle_user_input() => {}
            }
        }
    }

    async fn handle_question(&self, question: QuestionRequest) {
        let request_id = question.id.clone();
        let mut all_answers: Vec<QuestionAnswer> = Vec::new();

        for q in question.questions.iter() {
            display_question(q);

            if let Some(answer) = read_question_answer(q) {
                all_answers.push(answer);
                break;
            }
        }

        let final_response = QuestionResponse {
            request_id: request_id.clone(),
            answers: all_answers,
        };

        let _ = self
            .question_sender
            .submit_response(request_id, final_response)
            .await;
    }

    async fn handle_user_input(&mut self) {
        print!("{}User> {}", colors::GREEN, colors::RESET);
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            let line = input.trim().to_string();
            if !line.is_empty() {
                if let Some(cmd) = InputCommand::parse(&line) {
                    self.handle_command(cmd).await;
                } else {
                    self.handle_multimodal_input(line).await;
                }
            }
        }
    }

    async fn handle_command(&mut self, cmd: InputCommand) {
        match cmd {
            InputCommand::Attach(path) => {
                self.add_attachment(&path).await;
            }
            InputCommand::Attachments => {
                self.list_attachments();
            }
            InputCommand::ClearAttachments => {
                self.pending_attachments.clear();
                println!(
                    "{}Attachments cleared{}",
                    colors::GREEN,
                    colors::RESET
                );
            }
            InputCommand::History(_count) => {
                let _ = self.event_tx.send(InputEvent::Command(cmd));
            }
            _ => {
                let _ = self.event_tx.send(InputEvent::Command(cmd));
            }
        }
    }

    async fn add_attachment(&mut self, path: &str) {
        match MediaAttachment::from_path(Path::new(path)) {
            Ok(attachment) => {
                let media_type = match attachment.media_type {
                    MediaType::Image => "image",
                    MediaType::Audio => "audio",
                    MediaType::Video => "video",
                    MediaType::Pdf => "PDF",
                    MediaType::Unknown => "file",
                };
                self.pending_attachments.add(attachment);
                println!(
                    "{}{} attached: {}{}",
                    colors::GREEN,
                    media_type,
                    path,
                    colors::RESET
                );
            }
            Err(e) => {
                println!(
                    "{}Failed to attach file: {}{}",
                    colors::RED,
                    e,
                    colors::RESET
                );
            }
        }
    }

    fn list_attachments(&self) {
        if self.pending_attachments.is_empty() {
            println!("{}No attachments{}", colors::YELLOW, colors::RESET);
            return;
        }

        println!(
            "{}Attachments ({}):",
            colors::GREEN,
            self.pending_attachments.count()
        );
        for (i, attachment) in
            self.pending_attachments.list().iter().enumerate()
        {
            let media_type = match attachment.media_type {
                MediaType::Image => "[Image]",
                MediaType::Audio => "[Audio]",
                MediaType::Video => "[Video]",
                MediaType::Pdf => "[PDF]",
                MediaType::Unknown => "[File]",
            };
            let source = attachment
                .source_path
                .clone()
                .or_else(|| attachment.source_url.clone())
                .unwrap_or_else(|| "unknown".to_string());
            println!(
                "  {} {}{} ({})",
                i + 1,
                media_type,
                source,
                Self::format_size(attachment.size())
            );
        }
    }

    fn format_size(bytes: usize) -> String {
        if bytes < 1024 {
            format!("{}B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1}KB", bytes as f64 / 1024.0)
        } else if bytes < 1024 * 1024 * 1024 {
            format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
        } else {
            format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
        }
    }

    async fn handle_multimodal_input(&mut self, line: String) {
        match MediaProcessor::process_input(&line) {
            Ok((text, mut processed_attachments)) => {
                let mut all_attachments = self.pending_attachments.take();
                all_attachments.append(&mut processed_attachments);

                let _ = self.event_tx.send(InputEvent::MultimodalInput {
                    text,
                    attachments: all_attachments,
                });
            }
            Err(e) => {
                tracing::warn!("Failed to process multimodal input: {}", e);
                let _ = self.event_tx.send(InputEvent::MultimodalInput {
                    text: line,
                    attachments: vec![],
                });
            }
        }
    }
}

fn display_question(q: &Question) {
    println!();
    if !q.header.is_empty() {
        println!("{}", q.header);
    }
    println!("{}", q.question);
    println!();

    for (i, opt) in q.options.iter().enumerate() {
        println!("  {}. {} - {}", i + 1, opt.label, opt.description);
    }

    if q.multi_select {
        println!(
            "\n{}Multiple selection allowed (comma-separated){}",
            colors::YELLOW,
            colors::RESET
        );
    }
    println!();
}

fn read_question_answer(q: &Question) -> Option<QuestionAnswer> {
    print!("{}Your choice{} > ", colors::GREEN, colors::RESET);
    io::stdout().flush().ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return None;
    }

    let line = input.trim();
    InputCommand::parse_answer(line, q)
        .and_then(|resp| resp.answers.into_iter().next())
        .or_else(|| {
            println!(
                "{}Invalid selection, please try again{}",
                colors::RED,
                colors::RESET
            );
            None
        })
}

#[cfg(test)]
mod tests {
    use synthia_agent::tools::QuestionOption;

    use super::*;

    fn make_test_question(multi_select: bool) -> Question {
        Question {
            header: "Test Header".to_string(),
            question: "Test Question?".to_string(),
            options: vec![
                QuestionOption {
                    label: "Option A".to_string(),
                    description: "First option".to_string(),
                },
                QuestionOption {
                    label: "Option B".to_string(),
                    description: "Second option".to_string(),
                },
            ],
            multi_select,
        }
    }

    #[test]
    fn test_parse_single_select_valid() {
        let q = make_test_question(false);
        let result = InputCommand::parse_answer("1", &q);
        assert!(result.is_some());
        let resp = result.unwrap();
        assert_eq!(resp.answers[0].selected, vec!["Option A"]);
    }

    #[test]
    fn test_parse_single_select_other() {
        let q = make_test_question(false);
        let result = InputCommand::parse_answer("custom text", &q);
        assert!(result.is_some());
        let resp = result.unwrap();
        assert_eq!(resp.answers[0].other, Some("custom text".to_string()));
    }

    #[test]
    fn test_parse_multi_select_valid() {
        let q = make_test_question(true);
        let result = InputCommand::parse_answer("1, 2", &q);
        assert!(result.is_some());
        let resp = result.unwrap();
        assert_eq!(resp.answers[0].selected, vec!["Option A", "Option B"]);
    }

    #[test]
    fn test_parse_multi_select_invalid() {
        let q = make_test_question(true);
        let result = InputCommand::parse_answer("1, 5", &q);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_command_quit() {
        let cmd = InputCommand::parse("/quit");
        assert!(matches!(cmd, Some(InputCommand::Quit)));

        let cmd = InputCommand::parse("/exit");
        assert!(matches!(cmd, Some(InputCommand::Quit)));
    }

    #[test]
    fn test_parse_command_speed() {
        let cmd = InputCommand::parse("/speed 10");
        assert!(matches!(cmd, Some(InputCommand::Speed(10))));
    }

    #[test]
    fn test_parse_command_attach() {
        let cmd = InputCommand::parse("/attach /path/to/image.png");
        assert!(
            matches!(cmd, Some(InputCommand::Attach(path)) if path == "/path/to/image.png")
        );
    }

    #[test]
    fn test_parse_command_export() {
        let cmd = InputCommand::parse("/export json");
        assert!(
            matches!(cmd, Some(InputCommand::Export(format)) if format == "json")
        );
    }

    #[test]
    fn test_pending_attachments() {
        let mut attachments = PendingAttachments::new();
        assert!(attachments.is_empty());
        assert_eq!(attachments.count(), 0);

        attachments.clear();
        assert!(attachments.is_empty());
    }

    #[test]
    fn test_parse_command_session_new() {
        let cmd = InputCommand::parse("/session");
        assert!(matches!(cmd, Some(InputCommand::SessionNew)));
    }

    #[test]
    fn test_parse_command_session_switch() {
        let cmd = InputCommand::parse("/session abc-123");
        assert!(
            matches!(cmd, Some(InputCommand::SessionSwitch(id)) if id == "abc-123")
        );
    }

    #[test]
    fn test_parse_command_session_list() {
        let cmd = InputCommand::parse("/sessions");
        assert!(matches!(cmd, Some(InputCommand::SessionList)));
    }
}
