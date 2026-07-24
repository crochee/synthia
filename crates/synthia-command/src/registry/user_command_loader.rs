use std::{fs, path::Path};

use synthia_core::Error;

use crate::{
    traits::CommandHandler,
    types::{CommandContext, CommandResult},
};

const FRONTMATTER_DELIMITER: &str = "---";

#[derive(Debug, Clone)]
struct UserCommand {
    name: String,
    description: String,
    body: String,
}

impl UserCommand {
    fn from_toml(content: &str, _filename: &str) -> Option<Self> {
        let value: toml::Value = content.parse().ok()?;
        let name = value.get("name")?.as_str()?.to_string();
        let description = value
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let body = value
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Some(Self {
            name,
            description,
            body,
        })
    }

    fn from_markdown(content: &str, _filename: &str) -> Option<Self> {
        let parts: Vec<&str> =
            content.splitn(3, FRONTMATTER_DELIMITER).collect();
        if parts.len() < 3 {
            return None;
        }

        let frontmatter = parts[1].trim();
        let body = parts[2].trim().to_string();

        let metadata: serde_yaml::Value =
            serde_yaml::from_str(frontmatter).ok()?;
        let name = metadata.get("name")?.as_str()?.to_string();
        let description = metadata
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Some(Self {
            name,
            description,
            body,
        })
    }
}

#[derive(Clone)]
pub struct FileCommand {
    pub name: String,
    pub description: String,
    pub body: String,
}

#[async_trait::async_trait]
impl CommandHandler for FileCommand {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(
        &self,
        args: &str,
        _ctx: &CommandContext,
    ) -> Result<CommandResult, Error> {
        let output = if self.body.is_empty() {
            format!("User command '{}' executed. Args: {}", self.name, args)
        } else {
            format!(
                "User command '{}' executed.\n\nDescription: {}\n\nBody:\n{}\n\nArgs: {}",
                self.name, self.description, self.body, args
            )
        };
        Ok(CommandResult::new(output))
    }
}

pub fn load_user_command_file(path: &Path) -> Option<FileCommand> {
    let content = fs::read_to_string(path).ok()?;
    let filename = path.file_name()?.to_str()?;

    let user_cmd = if filename.ends_with(".toml") {
        UserCommand::from_toml(&content, filename)
    } else if filename.ends_with(".md") {
        UserCommand::from_markdown(&content, filename)
    } else {
        None
    }?;

    Some(FileCommand {
        name: user_cmd.name,
        description: user_cmd.description,
        body: user_cmd.body,
    })
}

pub fn load_commands_from_directory(dir: &Path) -> Vec<FileCommand> {
    if !dir.exists() {
        return Vec::new();
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut commands = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && let Some(cmd) = load_user_command_file(&path)
        {
            commands.push(cmd);
        }
    }

    commands
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_load_toml_command() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("greet.toml");
        fs::write(
            &path,
            r#"
name = "greet"
description = "Greet the user"
body = "Hello, how can I help you?"
"#,
        )
        .unwrap();

        let cmd = load_user_command_file(&path).unwrap();
        assert_eq!(cmd.name, "greet");
        assert_eq!(cmd.description, "Greet the user");
        assert_eq!(cmd.body, "Hello, how can I help you?");
    }

    #[test]
    fn test_load_markdown_command() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("hello.md");
        fs::write(
            &path,
            r#"---
name: hello
description: Say hello to the user
---
# Hello Command

This command says hello to the user.
The body content follows the YAML frontmatter.
"#,
        )
        .unwrap();

        let cmd = load_user_command_file(&path).unwrap();
        assert_eq!(cmd.name, "hello");
        assert_eq!(cmd.description, "Say hello to the user");
        assert!(cmd.body.contains("Hello Command"));
        assert!(cmd.body.contains("This command says hello"));
    }

    #[test]
    fn test_load_commands_from_directory() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        fs::write(
            dir.join("cmd1.toml"),
            r#"
name = "cmd1"
description = "First command"
"#,
        )
        .unwrap();

        fs::write(
            dir.join("cmd2.md"),
            r#"---
name: cmd2
description: Second command
---
Command body
"#,
        )
        .unwrap();

        let commands = load_commands_from_directory(dir);
        assert_eq!(commands.len(), 2);
        assert!(commands.iter().any(|c| c.name == "cmd1"));
        assert!(commands.iter().any(|c| c.name == "cmd2"));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result =
            load_user_command_file(Path::new("/nonexistent/file.toml"));
        assert!(result.is_none());
    }

    #[test]
    fn test_load_invalid_toml() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("invalid.toml");
        fs::write(&path, "not valid toml").unwrap();

        let result = load_user_command_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn test_load_invalid_markdown() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("invalid.md");
        fs::write(&path, "No frontmatter here").unwrap();

        let result = load_user_command_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn test_markdown_without_required_name() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("noname.md");
        fs::write(
            &path,
            r#"---
description: Has description but no name
---
Body content
"#,
        )
        .unwrap();

        let result = load_user_command_file(&path);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_file_command_execute() {
        let cmd = FileCommand {
            name: "test".to_string(),
            description: "Test command".to_string(),
            body: "Body content".to_string(),
        };
        let ctx = CommandContext::new(
            "s1".to_string(),
            std::path::PathBuf::from("/tmp"),
        );
        let result = cmd.execute("arg1 arg2", &ctx).await.unwrap();
        assert!(result.output.contains("test"));
        assert!(result.output.contains("Body content"));
        assert!(result.output.contains("arg1 arg2"));
    }
}
