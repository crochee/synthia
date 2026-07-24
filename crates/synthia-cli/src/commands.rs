#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentMode {
    /// Default mode: asks for approval on tool calls
    #[default]
    Interactive,
    /// Displays proposed tool calls but does NOT execute them
    Plan,
    /// Autonomous execution, no approval needed for allowed tools
    Execute,
    /// Requires explicit approval for EVERY tool call
    Review,
}

impl std::fmt::Display for AgentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentMode::Interactive => write!(f, "interactive"),
            AgentMode::Plan => write!(f, "plan"),
            AgentMode::Execute => write!(f, "execute"),
            AgentMode::Review => write!(f, "review"),
        }
    }
}

impl std::str::FromStr for AgentMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "interactive" | "default" => Ok(AgentMode::Interactive),
            "plan" => Ok(AgentMode::Plan),
            "execute" | "auto" => Ok(AgentMode::Execute),
            "review" => Ok(AgentMode::Review),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CliCommand {
    Exit,
    Quit,
    Help,
    Clear,
    Mode(Option<String>),
    Status,
    Compact,
    Model(Option<String>),
    Provider(Option<String>),
    Session(Option<String>),
    SessionList,
    SessionSwitch(String),
    SessionDelete(String),
    Tools,
    Memory(Option<String>),
    Skills,
    ConfigShow,
    ConfigReload,
    TaskList,
    SkillReport,
    SkillStats,
    Message(String),
    Unknown(String),
}

impl CliCommand {
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return CliCommand::Message(String::new());
        }

        if let Some(cmd) = trimmed.strip_prefix('/') {
            let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
            let command = parts[0].to_lowercase();
            let argument = parts
                .get(1)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            match command.as_str() {
                "exit" | "quit" => CliCommand::Exit,
                "q" => CliCommand::Quit,
                "help" | "h" => CliCommand::Help,
                "clear" => CliCommand::Clear,
                "mode" => CliCommand::Mode(argument),
                "status" => CliCommand::Status,
                "compact" => CliCommand::Compact,
                "model" | "m" => CliCommand::Model(argument),
                "provider" | "p" => CliCommand::Provider(argument),
                "session" | "s" => match argument.as_deref() {
                    Some("list" | "ls") => CliCommand::SessionList,
                    Some(arg) if arg.starts_with("switch ") => {
                        let id = arg["switch ".len()..].trim().to_string();
                        if id.is_empty() {
                            CliCommand::Unknown(
                                "session switch requires an id".to_string(),
                            )
                        } else {
                            CliCommand::SessionSwitch(id)
                        }
                    }
                    Some(arg) if arg.starts_with("delete ") => {
                        let id = arg["delete ".len()..].trim().to_string();
                        if id.is_empty() {
                            CliCommand::Unknown(
                                "session delete requires an id".to_string(),
                            )
                        } else {
                            CliCommand::SessionDelete(id)
                        }
                    }
                    Some("new") => CliCommand::Session(Some("new".to_string())),
                    Some(_) => CliCommand::Session(argument),
                    None => CliCommand::Session(None),
                },
                "tools" | "t" => CliCommand::Tools,
                "memory" => CliCommand::Memory(argument),
                "skills" => CliCommand::Skills,
                "config" => match argument.as_deref() {
                    Some("show") => CliCommand::ConfigShow,
                    Some("reload") => CliCommand::ConfigReload,
                    _ => CliCommand::Unknown(
                        "config: unknown subcommand, try 'show' or 'reload'"
                            .to_string(),
                    ),
                },
                "task" => match argument.as_deref() {
                    Some("list" | "ls") => CliCommand::TaskList,
                    _ => CliCommand::Unknown(
                        "task: unknown subcommand, try 'list'".to_string(),
                    ),
                },
                "skill" => match argument.as_deref() {
                    Some("report") => CliCommand::SkillReport,
                    Some("stats") => CliCommand::SkillStats,
                    _ => CliCommand::Unknown(
                        "skill: unknown subcommand, try 'report' or 'stats'"
                            .to_string(),
                    ),
                },
                other => CliCommand::Unknown(other.to_string()),
            }
        } else {
            CliCommand::Message(trimmed.to_string())
        }
    }

    pub fn is_exit(&self) -> bool {
        matches!(self, CliCommand::Exit | CliCommand::Quit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exit() {
        assert!(CliCommand::parse("/exit").is_exit());
        assert!(CliCommand::parse("/quit").is_exit());
    }

    #[test]
    fn test_parse_help() {
        assert_eq!(CliCommand::parse("/help"), CliCommand::Help);
        assert_eq!(CliCommand::parse("/h"), CliCommand::Help);
    }

    #[test]
    fn test_parse_model_no_arg() {
        assert_eq!(CliCommand::parse("/model"), CliCommand::Model(None));
        assert_eq!(CliCommand::parse("/m"), CliCommand::Model(None));
    }

    #[test]
    fn test_parse_model_with_arg() {
        assert_eq!(
            CliCommand::parse("/model gpt-4o"),
            CliCommand::Model(Some("gpt-4o".to_string()))
        );
    }

    #[test]
    fn test_parse_provider_no_arg() {
        assert_eq!(CliCommand::parse("/provider"), CliCommand::Provider(None));
    }

    #[test]
    fn test_parse_provider_with_arg() {
        assert_eq!(
            CliCommand::parse("/provider anthropic"),
            CliCommand::Provider(Some("anthropic".to_string()))
        );
    }

    #[test]
    fn test_parse_session_no_arg() {
        assert_eq!(CliCommand::parse("/session"), CliCommand::Session(None));
    }

    #[test]
    fn test_parse_session_new() {
        assert_eq!(
            CliCommand::parse("/session new"),
            CliCommand::Session(Some("new".to_string()))
        );
    }

    #[test]
    fn test_parse_tools() {
        assert_eq!(CliCommand::parse("/tools"), CliCommand::Tools);
        assert_eq!(CliCommand::parse("/t"), CliCommand::Tools);
    }

    #[test]
    fn test_parse_memory() {
        assert_eq!(CliCommand::parse("/memory"), CliCommand::Memory(None));
        assert_eq!(
            CliCommand::parse("/memory list"),
            CliCommand::Memory(Some("list".to_string()))
        );
    }

    #[test]
    fn test_parse_skills() {
        assert_eq!(CliCommand::parse("/skills"), CliCommand::Skills);
    }

    #[test]
    fn test_parse_message() {
        assert_eq!(
            CliCommand::parse("hello world"),
            CliCommand::Message("hello world".to_string())
        );
    }

    #[test]
    fn test_parse_unknown() {
        assert_eq!(
            CliCommand::parse("/unknown"),
            CliCommand::Unknown("unknown".to_string())
        );
    }

    #[test]
    fn test_parse_empty_input() {
        assert_eq!(CliCommand::parse(""), CliCommand::Message(String::new()));
    }

    #[test]
    fn test_parse_whitespace_only() {
        assert_eq!(
            CliCommand::parse("   "),
            CliCommand::Message(String::new())
        );
    }

    #[test]
    fn test_parse_model_case_insensitive() {
        assert_eq!(CliCommand::parse("/Model"), CliCommand::Model(None));
    }

    #[test]
    fn test_parse_memory_with_multiple_args() {
        assert_eq!(
            CliCommand::parse("/memory read my key"),
            CliCommand::Memory(Some("read my key".to_string()))
        );
    }

    #[test]
    fn test_parse_short_commands() {
        assert_eq!(CliCommand::parse("/q"), CliCommand::Quit);
        assert_eq!(CliCommand::parse("/h"), CliCommand::Help);
        assert_eq!(CliCommand::parse("/m"), CliCommand::Model(None));
        assert_eq!(CliCommand::parse("/p"), CliCommand::Provider(None));
        assert_eq!(CliCommand::parse("/s"), CliCommand::Session(None));
        assert_eq!(CliCommand::parse("/t"), CliCommand::Tools);
    }
}
