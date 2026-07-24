use anyhow::Result;

use super::PromptSection;
use crate::prompt::{PromptContext, SectionCaching};

#[derive(Debug, Clone, Default)]
pub struct EnvironmentSection;

impl EnvironmentSection {
    pub fn new() -> Self {
        Self
    }
}

impl PromptSection for EnvironmentSection {
    fn name(&self) -> &str {
        "environment"
    }

    fn caching(&self) -> SectionCaching {
        SectionCaching::Volatile
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let workspace_dir = ctx.workspace_dir;
        let work_dir = workspace_dir.display();
        let git_status = if workspace_dir.join(".git").exists() {
            "Yes"
        } else {
            "No"
        };
        let shell_info = std::env::var("SHELL")
            .map(|s| {
                if s.contains("zsh") {
                    "zsh"
                } else if s.contains("bash") {
                    "bash"
                } else {
                    "sh"
                }
            })
            .unwrap_or("unknown");
        let arch = std::env::consts::ARCH;
        let platform_info = match std::env::consts::OS {
            "windows" => "Windows (use Unix shell syntax)".to_string(),
            "macos" => "macOS".to_string(),
            "linux" => {
                let distro = std::env::var("WSL_DISTRO_NAME")
                    .or_else(|_| {
                        std::env::var("WSLENV").map(|_| "WSL".to_string())
                    })
                    .unwrap_or_default();
                if !distro.is_empty() {
                    format!("Linux ({distro}, WSL)")
                } else {
                    read_os_release()
                        .map(|s| format!("Linux ({s})"))
                        .unwrap_or_else(|| "Linux".to_string())
                }
            }
            other => other.to_string(),
        };
        let os_ver = if std::env::consts::OS == "windows" {
            String::from("Windows")
        } else {
            std::env::var("OSTYPE").unwrap_or_else(|_| String::from("unknown"))
        };

        let model_info = match (ctx.model_name, ctx.knowledge_cutoff) {
            (Some(name), Some(cutoff)) => {
                format!("Model: {name}\nKnowledge cutoff: {cutoff}")
            }
            (Some(name), None) => format!("Model: {name}"),
            (None, _) => String::new(),
        };

        let datetime = format!(
            "Current time: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );

        let mut output = format!(
            "# Environment\n\n<env>\nWorking directory: {work_dir}\nIs directory a git repo: {git_status}\nArchitecture: {arch}\nPlatform: {platform_info}\nShell: {shell_info}\nOS Version: {os_ver}\n{datetime}"
        );

        if !model_info.is_empty() {
            output.push_str(&format!("\n{model_info}"));
        }

        if git_status == "Yes" {
            if let Some(branch_name) = get_git_branch(workspace_dir) {
                output.push_str(&format!("\nGit branch: {branch_name}"));
            }
            if let Some(remote_url) = get_git_remote(workspace_dir) {
                output.push_str(&format!("\nGit remote: {remote_url}"));
            }
        }

        output.push_str("\n</env>");

        if !ctx.additional_dirs.is_empty() {
            let dirs: Vec<String> = ctx
                .additional_dirs
                .iter()
                .map(|p| p.display().to_string())
                .collect();
            output.push_str(&format!(
                "\n\nAdditional working directories: {}\n",
                dirs.join(", ")
            ));
        }

        Ok(output)
    }
}

fn read_os_release() -> Option<String> {
    let path = "/etc/os-release";
    std::fs::read_to_string(path)
        .ok()?
        .lines()
        .find(|line| line.starts_with("PRETTY_NAME="))
        .and_then(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|s| s.trim_matches('"').to_string())
}

fn get_git_branch(workspace_dir: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(workspace_dir)
        .output()
        .ok()?;
    if output.status.success() {
        let branch = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if !branch.is_empty() && branch != "HEAD" {
            return Some(branch);
        }
    }
    None
}

fn get_git_remote(workspace_dir: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(workspace_dir)
        .output()
        .ok()?;
    if output.status.success() {
        let remote = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if !remote.is_empty() {
            return Some(remote);
        }
    }
    None
}
