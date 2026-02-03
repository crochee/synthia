//! Todo rendering module
//!
//! Provides rendering functionality for todo lists.

use std::collections::HashMap;

use super::types::{TodoItem, TodoStatus};

const MARKER_COMPLETED: &str = "[✔]";
const MARKER_IN_PROGRESS: &str = "[>]";
const MARKER_PENDING: &str = "[ ]";
const MARKER_CANCELLED: &str = "[x]";

#[derive(Debug, Clone)]
pub(crate) struct TodoRenderer<'a> {
    items: &'a [TodoItem],
    id_to_content: HashMap<&'a str, &'a str>,
}

impl<'a> TodoRenderer<'a> {
    pub(crate) fn new(items: &'a [TodoItem]) -> Self {
        let id_to_content = items
            .iter()
            .map(|item| (item.id.as_str(), item.content.as_str()))
            .collect();
        Self {
            items,
            id_to_content,
        }
    }

    pub(crate) fn render(&self) -> String {
        if self.items.is_empty() {
            return "No todos.".to_string();
        }

        let mut lines = Vec::new();
        lines.push("# Task List".to_string());
        lines.push(String::new());

        for item in self.items.iter() {
            lines.push(self.render_item(item));
        }

        lines.push(String::new());
        lines.push(self.render_progress());

        lines.join("\n")
    }

    fn render_item(&self, item: &TodoItem) -> String {
        let marker = Self::get_marker(item.status);
        let mut line = format!("{} {} {}", item.id, marker, item.content);

        if item.status == TodoStatus::InProgress
            && let Some(active_forms) = item.active_form.as_deref()
            && !active_forms.is_empty()
        {
            line = format!(
                "{} {} {} <- {}",
                item.id,
                marker,
                item.content,
                active_forms.join(", ")
            );
        }

        if let Some(deps) = item.deps.as_deref()
            && !deps.is_empty()
        {
            let dep_contents = self.resolve_deps(deps);
            line.push_str(&format!(" (deps: {dep_contents})"));
        }

        line
    }

    fn get_marker(status: TodoStatus) -> &'static str {
        match status {
            TodoStatus::Completed => MARKER_COMPLETED,
            TodoStatus::InProgress => MARKER_IN_PROGRESS,
            TodoStatus::Pending => MARKER_PENDING,
            TodoStatus::Cancelled => MARKER_CANCELLED,
        }
    }

    fn resolve_deps(&self, deps: &[String]) -> String {
        let resolved: Vec<&str> = deps
            .iter()
            .filter_map(|dep_id| {
                self.id_to_content.get(dep_id.as_str()).copied()
            })
            .collect();

        if resolved.is_empty() {
            deps.join(", ")
        } else {
            resolved.join(", ")
        }
    }

    fn render_progress(&self) -> String {
        let completed = self
            .items
            .iter()
            .filter(|item| matches!(item.status, TodoStatus::Completed))
            .count();
        let total = self.items.len();
        let percentage = (completed as f64 / total as f64) * 100.0;

        format!("Progress: {completed}/{total} completed ({percentage:.1}%)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_items() -> Vec<TodoItem> {
        vec![
            TodoItem {
                id: "task-1".to_string(),
                content: "Task 1".to_string(),
                status: TodoStatus::Completed,
                active_form: None,
                deps: None,
            },
            TodoItem {
                id: "task-2".to_string(),
                content: "Task 2".to_string(),
                status: TodoStatus::InProgress,
                active_form: Some(vec!["Working on task 2".to_string()]),
                deps: Some(vec!["task-1".to_string()]),
            },
            TodoItem {
                id: "task-3".to_string(),
                content: "Task 3".to_string(),
                status: TodoStatus::Pending,
                active_form: None,
                deps: Some(vec!["task-1".to_string(), "task-2".to_string()]),
            },
        ]
    }

    #[test]
    fn test_render_empty() {
        let renderer = TodoRenderer::new(&[]);
        assert_eq!(renderer.render(), "No todos.");
    }

    #[test]
    fn test_render_with_items() {
        let items = create_test_items();
        let renderer = TodoRenderer::new(&items);
        let output = renderer.render();

        assert!(output.contains("# Task List"));
        assert!(output.contains("[✔]"));
        assert!(output.contains("[>]"));
        assert!(output.contains("[ ]"));
        assert!(output.contains("Progress: 1/3"));
        assert!(output.contains("task-1"));
        assert!(output.contains("task-2"));
        assert!(output.contains("task-3"));
    }

    #[test]
    fn test_resolve_deps() {
        let items = create_test_items();
        let renderer = TodoRenderer::new(&items);

        let resolved = renderer
            .resolve_deps(&["task-1".to_string(), "task-2".to_string()]);
        assert_eq!(resolved, "Task 1, Task 2");
    }

    #[test]
    fn test_resolve_deps_unknown() {
        let items = create_test_items();
        let renderer = TodoRenderer::new(&items);

        let resolved = renderer.resolve_deps(&["unknown-id".to_string()]);
        assert_eq!(resolved, "unknown-id");
    }
}
