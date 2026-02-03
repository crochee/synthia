//! Worktree index management

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{WorktreeEntry, WorktreeEventBus};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct WorktreeIndex {
    pub worktrees: Vec<WorktreeEntry>,
}

#[derive(Debug)]
pub(crate) struct WorktreeManager {
    repo_root: PathBuf,
    worktrees_dir: PathBuf,
    index_path: PathBuf,
    event_bus: WorktreeEventBus,
}

impl WorktreeManager {
    pub(crate) fn new(repo_root: PathBuf) -> Self {
        let worktrees_dir = repo_root.join(".agents/worktrees");
        let index_path = worktrees_dir.join("index.json");
        let event_bus = WorktreeEventBus::new(worktrees_dir.clone());

        std::fs::create_dir_all(&worktrees_dir).ok();

        if !index_path.exists() {
            let index = WorktreeIndex::default();
            std::fs::write(
                &index_path,
                serde_json::to_string_pretty(&index).unwrap_or_default(),
            )
            .ok();
        }

        Self {
            repo_root,
            worktrees_dir,
            index_path,
            event_bus,
        }
    }

    fn load_index(&self) -> WorktreeIndex {
        std::fs::read_to_string(&self.index_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default()
    }

    fn save_index(&self, index: &WorktreeIndex) -> std::io::Result<()> {
        std::fs::write(
            &self.index_path,
            serde_json::to_string_pretty(index).unwrap_or_default(),
        )
    }

    pub(crate) fn find(&self, name: &str) -> Option<WorktreeEntry> {
        self.load_index()
            .worktrees
            .into_iter()
            .find(|wt| wt.name == name)
    }

    pub(crate) fn create(
        &self,
        name: &str,
        task_id: Option<i64>,
        base_ref: &str,
    ) -> Result<WorktreeEntry, String> {
        if !name.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'
        }) {
            return Err("Invalid worktree name. Use letters, numbers, ., _, -"
                .to_string());
        }

        if self.find(name).is_some() {
            return Err(format!("Worktree '{name}' already exists"));
        }

        let path = self.worktrees_dir.join(name);
        let branch = format!("wt/{name}");

        self.event_bus
            .emit(
                "worktree.create.before",
                task_id.map(|id| serde_json::json!({"id": id})),
                Some(serde_json::json!({"name": name, "base_ref": base_ref})),
                None,
            )
            .map_err(|e| e.to_string())?;

        let output = std::process::Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch,
                path.to_str().unwrap_or(""),
                base_ref,
            ])
            .current_dir(&self.repo_root)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let entry = WorktreeEntry {
                    name: name.to_string(),
                    path: path.to_string_lossy().to_string(),
                    branch,
                    task_id,
                    status: "active".to_string(),
                    created_at: chrono::Utc::now().timestamp(),
                };

                let mut index = self.load_index();
                index.worktrees.push(entry.clone());
                self.save_index(&index).map_err(|e| e.to_string())?;

                self.event_bus
                    .emit(
                        "worktree.create.after",
                        task_id.map(|id| serde_json::json!({"id": id})),
                        Some(serde_json::json!({
                            "name": entry.name,
                            "path": entry.path,
                            "branch": entry.branch,
                            "status": entry.status
                        })),
                        None,
                    )
                    .ok();

                Ok(entry)
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                self.event_bus.emit(
                    "worktree.create.failed",
                    task_id.map(|id| serde_json::json!({"id": id})),
                    Some(serde_json::json!({"name": name, "base_ref": base_ref})),
                    Some(err.to_string()),
                ).ok();
                Err(err.to_string())
            }
            Err(e) => {
                self.event_bus.emit(
                    "worktree.create.failed",
                    task_id.map(|id| serde_json::json!({"id": id})),
                    Some(serde_json::json!({"name": name, "base_ref": base_ref})),
                    Some(e.to_string()),
                ).ok();
                Err(e.to_string())
            }
        }
    }

    pub(crate) fn list(&self) -> Vec<WorktreeEntry> {
        self.load_index().worktrees
    }

    pub(crate) fn status(&self, name: &str) -> Result<String, String> {
        let wt = self
            .find(name)
            .ok_or_else(|| format!("Worktree '{name}' not found"))?;
        let path = PathBuf::from(&wt.path);

        if !path.exists() {
            return Ok(format!("Error: Worktree path missing: {}", wt.path));
        }

        let output = std::process::Command::new("git")
            .args(["status", "--short", "--branch"])
            .current_dir(&path)
            .output()
            .map_err(|e| e.to_string())?;

        let text = String::from_utf8_lossy(&output.stdout);
        Ok(if text.is_empty() {
            "Clean worktree".to_string()
        } else {
            text.to_string()
        })
    }

    pub(crate) fn run(
        &self,
        name: &str,
        command: &str,
    ) -> Result<String, String> {
        let dangerous = ["rm -rf /", "sudo", "shutdown", "reboot", "> /dev/"];
        if dangerous.iter().any(|d| command.contains(d)) {
            return Err("Dangerous command blocked".to_string());
        }

        let wt = self
            .find(name)
            .ok_or_else(|| format!("Worktree '{name}' not found"))?;
        let path = PathBuf::from(&wt.path);

        if !path.exists() {
            return Err(format!("Worktree path missing: {}", wt.path));
        }

        let output = std::process::Command::new("sh")
            .args(["-c", command])
            .current_dir(&path)
            .output()
            .map_err(|e| e.to_string())?;

        let text = String::from_utf8_lossy(&output.stdout);
        let err = String::from_utf8_lossy(&output.stderr);
        let result = if text.is_empty() {
            err.to_string()
        } else {
            text.to_string()
        };

        Ok(result.chars().take(50000).collect())
    }

    pub(crate) fn remove(
        &self,
        name: &str,
        force: bool,
    ) -> Result<String, String> {
        let wt = self
            .find(name)
            .ok_or_else(|| format!("Worktree '{name}' not found"))?;

        self.event_bus
            .emit(
                "worktree.remove.before",
                wt.task_id.map(|id| serde_json::json!({"id": id})),
                Some(serde_json::json!({"name": name, "path": wt.path})),
                None,
            )
            .ok();

        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&wt.path);

        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(&self.repo_root)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let mut index = self.load_index();
                if let Some(entry) =
                    index.worktrees.iter_mut().find(|e| e.name == name)
                {
                    entry.status = "removed".to_string();
                }
                self.save_index(&index).ok();

                self.event_bus.emit(
                    "worktree.remove.after",
                    wt.task_id.map(|id| serde_json::json!({"id": id})),
                    Some(serde_json::json!({"name": name, "path": wt.path, "status": "removed"})),
                    None,
                ).ok();

                Ok(format!("Removed worktree '{name}'"))
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr).to_string();
                self.event_bus
                    .emit(
                        "worktree.remove.failed",
                        wt.task_id.map(|id| serde_json::json!({"id": id})),
                        Some(
                            serde_json::json!({"name": name, "path": wt.path}),
                        ),
                        Some(err.clone()),
                    )
                    .ok();
                Err(err)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    pub(crate) fn keep(&self, name: &str) -> Result<WorktreeEntry, String> {
        let mut index = self.load_index();

        let entry = index
            .worktrees
            .iter_mut()
            .find(|e| e.name == name)
            .ok_or_else(|| format!("Worktree '{name}' not found"))?;

        entry.status = "kept".to_string();
        let entry = entry.clone();

        self.save_index(&index).map_err(|e| e.to_string())?;

        self.event_bus.emit(
            "worktree.keep",
            entry.task_id.map(|id| serde_json::json!({"id": id})),
            Some(serde_json::json!({"name": name, "path": entry.path, "status": "kept"})),
            None,
        ).ok();

        Ok(entry)
    }

    pub(crate) fn events(&self, limit: usize) -> String {
        self.event_bus
            .list_recent(limit)
            .unwrap_or_else(|_| "[]".to_string())
    }
}
