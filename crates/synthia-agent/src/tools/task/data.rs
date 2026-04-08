use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
    Stopped,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Stopped => "stopped",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "in_progress" => TaskStatus::InProgress,
            "completed" => TaskStatus::Completed,
            "failed" => TaskStatus::Failed,
            "stopped" => TaskStatus::Stopped,
            _ => TaskStatus::Pending,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Stopped
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum TaskPriority {
    #[default]
    Normal,
    Low,
    High,
    Critical,
}

impl TaskPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskPriority::Low => "low",
            TaskPriority::Normal => "normal",
            TaskPriority::High => "high",
            TaskPriority::Critical => "critical",
        }
    }

    pub fn from_db_string(s: &str) -> Self {
        match s {
            "low" => TaskPriority::Low,
            "high" => TaskPriority::High,
            "critical" => TaskPriority::Critical,
            _ => TaskPriority::Normal,
        }
    }

    pub fn level(&self) -> u8 {
        match self {
            TaskPriority::Low => 1,
            TaskPriority::Normal => 2,
            TaskPriority::High => 3,
            TaskPriority::Critical => 4,
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskPacket {
    pub objective: String,
    pub scope: String,
    pub repo: String,
    pub branch_policy: String,
    pub acceptance_tests: Vec<String>,
    pub commit_policy: String,
    pub reporting_contract: String,
    pub escalation_policy: String,
}

impl TaskPacket {
    pub fn new(objective: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            objective: objective.into(),
            scope: scope.into(),
            repo: String::new(),
            branch_policy: String::new(),
            acceptance_tests: Vec::new(),
            commit_policy: String::new(),
            reporting_contract: String::new(),
            escalation_policy: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskMessage {
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

impl TaskMessage {
    pub fn new(
        role: impl Into<String>,
        content: impl Into<String>,
        timestamp: i64,
    ) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            timestamp,
        }
    }

    pub fn user(content: impl Into<String>, timestamp: i64) -> Self {
        Self::new("user", content, timestamp)
    }

    pub fn assistant(content: impl Into<String>, timestamp: i64) -> Self {
        Self::new("assistant", content, timestamp)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
    pub owner: String,
    pub team_id: Option<String>,
    pub priority: TaskPriority,
    pub task_packet: Option<TaskPacket>,
    pub deadline: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub output: String,
    pub messages: Vec<TaskMessage>,
}

impl Task {
    pub fn new(id: impl Into<String>, subject: impl Into<String>) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: id.into(),
            subject: subject.into(),
            description: String::new(),
            status: TaskStatus::default(),
            blocked_by: Vec::new(),
            blocks: Vec::new(),
            owner: String::new(),
            team_id: None,
            priority: TaskPriority::default(),
            task_packet: None,
            deadline: None,
            created_at: now,
            updated_at: now,
            output: String::new(),
            messages: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = owner.into();
        self
    }

    pub fn with_team(mut self, team_id: impl Into<String>) -> Self {
        self.team_id = Some(team_id.into());
        self
    }

    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_deadline(mut self, deadline: i64) -> Self {
        self.deadline = Some(deadline);
        self
    }

    pub fn is_blocked(&self) -> bool {
        !self.blocked_by.is_empty()
    }

    pub fn is_assigned(&self) -> bool {
        !self.owner.is_empty()
    }

    pub fn add_message(
        &mut self,
        role: impl Into<String>,
        content: impl Into<String>,
    ) {
        let now = chrono::Utc::now().timestamp();
        self.messages.push(TaskMessage::new(role, content, now));
        self.updated_at = now;
    }

    pub fn append_output(&mut self, output: &str) {
        self.output.push_str(output);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct TaskPatch {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    pub blocked_by: Option<Vec<String>>,
    pub blocks: Option<Vec<String>>,
    pub owner: Option<String>,
    pub team_id: Option<String>,
    pub priority: Option<TaskPriority>,
    pub deadline: Option<i64>,
    pub output: Option<String>,
}

impl TaskPatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    pub fn with_team(mut self, team_id: impl Into<String>) -> Self {
        self.team_id = Some(team_id.into());
        self
    }
}

/// Represents a node in the task graph with its dependencies and priority.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TaskNode {
    /// The task ID
    pub id: String,
    /// IDs of tasks that this task depends on (must complete before this task)
    pub blocked_by: Vec<String>,
    /// IDs of tasks that depend on this task (this task must complete before them)
    pub blocks: Vec<String>,
    /// Task priority level
    pub priority: TaskPriority,
    /// Current status of the task
    pub status: TaskStatus,
}

impl TaskNode {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            blocked_by: Vec::new(),
            blocks: Vec::new(),
            priority: TaskPriority::default(),
            status: TaskStatus::default(),
        }
    }

    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_blocked_by(mut self, blocked_by: Vec<String>) -> Self {
        self.blocked_by = blocked_by;
        self
    }

    pub fn with_blocks(mut self, blocks: Vec<String>) -> Self {
        self.blocks = blocks;
        self
    }
}

impl From<Task> for TaskNode {
    fn from(task: Task) -> Self {
        Self {
            id: task.id,
            blocked_by: task.blocked_by,
            blocks: task.blocks,
            priority: task.priority,
            status: task.status,
        }
    }
}

impl From<&Task> for TaskNode {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id.clone(),
            blocked_by: task.blocked_by.clone(),
            blocks: task.blocks.clone(),
            priority: task.priority,
            status: task.status,
        }
    }
}

/// Result of a topological sort operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopologicalSortResult {
    /// Successfully sorted tasks in dependency order.
    Sorted(Vec<String>),
    /// Cycle detected in the task dependencies.
    CycleDetected(Vec<String>),
}

/// A graph structure for managing task dependencies.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct TaskGraph {
    /// Map of task ID to task node
    nodes: HashMap<String, TaskNode>,
}

impl TaskGraph {
    /// Creates a new empty task graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Adds a task node to the graph.
    pub fn add_task(&mut self, task: TaskNode) {
        self.nodes.insert(task.id.clone(), task);
    }

    /// Adds a task from an existing Task struct.
    pub fn add_task_from_struct(&mut self, task: Task) {
        self.add_task(TaskNode::from(task));
    }

    /// Removes a task from the graph.
    pub fn remove_task(&mut self, task_id: &str) -> Option<TaskNode> {
        self.nodes.remove(task_id)
    }

    /// Gets a task by ID.
    pub fn get_task(&self, task_id: &str) -> Option<&TaskNode> {
        self.nodes.get(task_id)
    }

    /// Gets a mutable reference to a task by ID.
    pub fn get_task_mut(&mut self, task_id: &str) -> Option<&mut TaskNode> {
        self.nodes.get_mut(task_id)
    }

    /// Returns all task IDs in the graph.
    pub fn task_ids(&self) -> Vec<&String> {
        self.nodes.keys().collect()
    }

    /// Returns the number of tasks in the graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns true if the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Adds a dependency edge: `from` task must complete before `to` task can start.
    /// Returns false if either task doesn't exist.
    pub fn add_dependency(&mut self, from: &str, to: &str) -> bool {
        if !self.nodes.contains_key(from) || !self.nodes.contains_key(to) {
            return false;
        }

        // Add to blocked_by of the dependent task
        if let Some(to_task) = self.nodes.get_mut(to)
            && !to_task.blocked_by.contains(&from.to_string())
        {
            to_task.blocked_by.push(from.to_string());
        }

        // Add to blocks of the prerequisite task
        if let Some(from_task) = self.nodes.get_mut(from)
            && !from_task.blocks.contains(&to.to_string())
        {
            from_task.blocks.push(to.to_string());
        }

        true
    }

    /// Removes a dependency edge.
    pub fn remove_dependency(&mut self, from: &str, to: &str) {
        if let Some(to_task) = self.nodes.get_mut(to) {
            to_task.blocked_by.retain(|id| id != from);
        }
        if let Some(from_task) = self.nodes.get_mut(from) {
            from_task.blocks.retain(|id| id != to);
        }
    }

    /// Updates the status of a task.
    pub fn update_status(&mut self, task_id: &str, status: TaskStatus) -> bool {
        if let Some(task) = self.nodes.get_mut(task_id) {
            task.status = status;
            true
        } else {
            false
        }
    }

    /// Checks if all dependencies of a task are completed.
    /// Returns true if the task has no dependencies or all dependencies are completed.
    /// Returns false if the task doesn't exist or has incomplete dependencies.
    pub fn check_dependencies(&self, task_id: &str) -> bool {
        match self.nodes.get(task_id) {
            Some(task) => task.blocked_by.iter().all(|dep_id| {
                self.nodes
                    .get(dep_id)
                    .map(|dep| dep.status == TaskStatus::Completed)
                    .unwrap_or(false)
            }),
            None => false,
        }
    }

    /// Gets all tasks that are ready to execute (all dependencies completed).
    /// Only returns tasks that are in Pending status.
    pub fn get_ready_tasks(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|(_, task)| {
                task.status == TaskStatus::Pending
                    && self.check_dependencies(&task.id)
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Gets all tasks that are ready to execute, sorted by priority (highest first).
    pub fn get_ready_tasks_by_priority(&self) -> Vec<String> {
        let mut ready = self.get_ready_tasks();
        ready.sort_by(|a, b| {
            let task_a = self.nodes.get(a);
            let task_b = self.nodes.get(b);
            let priority_a = task_a.map(|t| t.priority.level()).unwrap_or(0);
            let priority_b = task_b.map(|t| t.priority.level()).unwrap_or(0);
            priority_b.cmp(&priority_a) // Higher priority first
        });
        ready
    }

    /// Gets all tasks that are blocked by a specific task.
    pub fn get_blocked_tasks(&self, task_id: &str) -> Vec<String> {
        self.nodes
            .get(task_id)
            .map(|task| task.blocks.clone())
            .unwrap_or_default()
    }

    /// Gets all tasks that block a specific task.
    pub fn get_blocking_tasks(&self, task_id: &str) -> Vec<String> {
        self.nodes
            .get(task_id)
            .map(|task| task.blocked_by.clone())
            .unwrap_or_default()
    }

    /// Performs topological sort using Kahn's algorithm.
    /// Returns Sorted with task IDs in dependency order, or CycleDetected with the cycle members.
    pub fn topological_sort(&self) -> TopologicalSortResult {
        if self.nodes.is_empty() {
            return TopologicalSortResult::Sorted(Vec::new());
        }

        // Calculate in-degree for each node
        let mut in_degree: HashMap<&String, usize> = HashMap::new();
        for task_id in self.nodes.keys() {
            in_degree.insert(task_id, 0);
        }

        for task in self.nodes.values() {
            for blocked_by in &task.blocked_by {
                // Only count dependencies that exist in the graph
                if self.nodes.contains_key(blocked_by)
                    && let Some(degree) = in_degree.get_mut(&task.id)
                {
                    *degree += 1;
                }
            }
        }

        // Find all nodes with no incoming edges (in-degree 0)
        let mut queue: Vec<&String> = in_degree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| *id)
            .collect();

        // Sort by priority (higher priority first) for deterministic output
        queue.sort_by(|a, b| {
            let priority_a =
                self.nodes.get(*a).map(|t| t.priority.level()).unwrap_or(0);
            let priority_b =
                self.nodes.get(*b).map(|t| t.priority.level()).unwrap_or(0);
            priority_b.cmp(&priority_a)
        });

        let mut sorted: Vec<String> = Vec::new();
        let mut visited_count = 0;

        while let Some(current) = queue.pop() {
            sorted.push(current.clone());
            visited_count += 1;

            // Get tasks that depend on current task
            if let Some(task) = self.nodes.get(current) {
                for blocked_task_id in &task.blocks {
                    if let Some(degree) = in_degree.get_mut(blocked_task_id) {
                        *degree -= 1;
                        if *degree == 0 {
                            // Insert in sorted position by priority
                            let priority = self
                                .nodes
                                .get(blocked_task_id)
                                .map(|t| t.priority.level())
                                .unwrap_or(0);
                            let pos = queue
                                .binary_search_by(|probe| {
                                    let probe_priority = self
                                        .nodes
                                        .get(*probe)
                                        .map(|t| t.priority.level())
                                        .unwrap_or(0);
                                    priority.cmp(&probe_priority)
                                })
                                .unwrap_or_else(|e| e);
                            queue.insert(pos, blocked_task_id);
                        }
                    }
                }
            }
        }

        if visited_count == self.nodes.len() {
            TopologicalSortResult::Sorted(sorted)
        } else {
            // Cycle detected - find the cycle members
            let cycle_members: Vec<String> = self
                .nodes
                .keys()
                .filter(|id| !sorted.contains(id))
                .cloned()
                .collect();
            TopologicalSortResult::CycleDetected(cycle_members)
        }
    }

    /// Detects if there are any cycles in the task graph.
    pub fn has_cycle(&self) -> bool {
        matches!(
            self.topological_sort(),
            TopologicalSortResult::CycleDetected(_)
        )
    }

    /// Finds all tasks involved in cycles.
    /// Returns an empty vector if no cycles exist.
    pub fn find_cycles(&self) -> Vec<String> {
        match self.topological_sort() {
            TopologicalSortResult::CycleDetected(cycle_members) => {
                cycle_members
            }
            TopologicalSortResult::Sorted(_) => Vec::new(),
        }
    }

    /// Gets the critical path through the task graph.
    /// Returns tasks that form the longest dependency chain.
    pub fn get_critical_path(&self) -> Vec<String> {
        // Use DFS to find the longest path
        let mut longest_path: Vec<String> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();

        for task_id in self.nodes.keys() {
            let path = self.dfs_longest_path(task_id, &mut visited);
            if path.len() > longest_path.len() {
                longest_path = path;
            }
        }

        longest_path
    }

    /// DFS helper to find the longest path starting from a given task.
    fn dfs_longest_path(
        &self,
        task_id: &str,
        visited: &mut HashSet<String>,
    ) -> Vec<String> {
        if visited.contains(task_id) {
            return Vec::new();
        }

        visited.insert(task_id.to_string());

        let mut longest: Vec<String> = vec![task_id.to_string()];

        if let Some(task) = self.nodes.get(task_id) {
            for blocked_task in &task.blocks {
                let path = self.dfs_longest_path(blocked_task, visited);
                if path.len() + 1 > longest.len() {
                    longest = vec![task_id.to_string()];
                    longest.extend(path);
                }
            }
        }

        visited.remove(task_id);
        longest
    }

    /// Gets tasks grouped by their depth in the dependency tree.
    /// Tasks at depth 0 have no dependencies, depth 1 tasks depend only on depth 0, etc.
    pub fn get_tasks_by_depth(&self) -> Vec<Vec<String>> {
        let mut depths: HashMap<String, usize> = HashMap::new();
        let mut result: Vec<Vec<String>> = Vec::new();

        // Calculate depth for each task
        for task_id in self.nodes.keys() {
            let depth = self.calculate_depth(task_id, &mut depths);
            while result.len() <= depth {
                result.push(Vec::new());
            }
            result[depth].push(task_id.clone());
        }

        result
    }

    /// Calculates the depth of a task in the dependency tree.
    fn calculate_depth(
        &self,
        task_id: &str,
        depths: &mut HashMap<String, usize>,
    ) -> usize {
        if let Some(&depth) = depths.get(task_id) {
            return depth;
        }

        let task = match self.nodes.get(task_id) {
            Some(t) => t,
            None => return 0,
        };

        if task.blocked_by.is_empty() {
            depths.insert(task_id.to_string(), 0);
            return 0;
        }

        let max_dep_depth = task
            .blocked_by
            .iter()
            .filter_map(|dep| {
                if self.nodes.contains_key(dep) {
                    Some(self.calculate_depth(dep, depths))
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0);

        let depth = max_dep_depth + 1;
        depths.insert(task_id.to_string(), depth);
        depth
    }

    /// Clears all tasks from the graph.
    pub fn clear(&mut self) {
        self.nodes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // TaskStatus tests
    // =====================================================================

    #[test]
    fn test_task_status_as_str() {
        assert_eq!(TaskStatus::Pending.as_str(), "pending");
        assert_eq!(TaskStatus::InProgress.as_str(), "in_progress");
        assert_eq!(TaskStatus::Completed.as_str(), "completed");
        assert_eq!(TaskStatus::Failed.as_str(), "failed");
        assert_eq!(TaskStatus::Stopped.as_str(), "stopped");
    }

    #[test]
    fn test_task_status_from_db_string() {
        assert_eq!(
            TaskStatus::from_db_string("in_progress"),
            TaskStatus::InProgress
        );
        assert_eq!(
            TaskStatus::from_db_string("completed"),
            TaskStatus::Completed
        );
        assert_eq!(TaskStatus::from_db_string("failed"), TaskStatus::Failed);
        assert_eq!(TaskStatus::from_db_string("stopped"), TaskStatus::Stopped);
    }

    #[test]
    fn test_task_status_from_db_string_unknown_defaults_to_pending() {
        assert_eq!(TaskStatus::from_db_string("unknown"), TaskStatus::Pending);
        assert_eq!(TaskStatus::from_db_string(""), TaskStatus::Pending);
        assert_eq!(TaskStatus::from_db_string("invalid"), TaskStatus::Pending);
    }

    #[test]
    fn test_task_status_is_terminal() {
        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::InProgress.is_terminal());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Stopped.is_terminal());
    }

    #[test]
    fn test_task_status_display() {
        assert_eq!(format!("{}", TaskStatus::Pending), "pending");
        assert_eq!(format!("{}", TaskStatus::Completed), "completed");
    }

    #[test]
    fn test_task_status_default() {
        let status = TaskStatus::default();
        assert_eq!(status, TaskStatus::Pending);
    }

    #[test]
    fn test_task_status_eq() {
        assert_eq!(TaskStatus::Pending, TaskStatus::Pending);
        assert_ne!(TaskStatus::Pending, TaskStatus::InProgress);
        assert_eq!(TaskStatus::Completed, TaskStatus::Completed);
    }

    // =====================================================================
    // TaskPriority tests
    // =====================================================================

    #[test]
    fn test_task_priority_as_str() {
        assert_eq!(TaskPriority::Low.as_str(), "low");
        assert_eq!(TaskPriority::Normal.as_str(), "normal");
        assert_eq!(TaskPriority::High.as_str(), "high");
        assert_eq!(TaskPriority::Critical.as_str(), "critical");
    }

    #[test]
    fn test_task_priority_from_db_string() {
        assert_eq!(TaskPriority::from_db_string("low"), TaskPriority::Low);
        assert_eq!(TaskPriority::from_db_string("high"), TaskPriority::High);
        assert_eq!(
            TaskPriority::from_db_string("critical"),
            TaskPriority::Critical
        );
    }

    #[test]
    fn test_task_priority_from_db_string_unknown_defaults_to_normal() {
        assert_eq!(
            TaskPriority::from_db_string("unknown"),
            TaskPriority::Normal
        );
        assert_eq!(TaskPriority::from_db_string(""), TaskPriority::Normal);
        assert_eq!(
            TaskPriority::from_db_string("urgent"),
            TaskPriority::Normal
        );
    }

    #[test]
    fn test_task_priority_level() {
        assert_eq!(TaskPriority::Low.level(), 1);
        assert_eq!(TaskPriority::Normal.level(), 2);
        assert_eq!(TaskPriority::High.level(), 3);
        assert_eq!(TaskPriority::Critical.level(), 4);
    }

    #[test]
    fn test_task_priority_display() {
        assert_eq!(format!("{}", TaskPriority::Low), "low");
        assert_eq!(format!("{}", TaskPriority::Critical), "critical");
    }

    #[test]
    fn test_task_priority_default() {
        let priority = TaskPriority::default();
        assert_eq!(priority, TaskPriority::Normal);
    }

    // =====================================================================
    // TaskPacket tests
    // =====================================================================

    #[test]
    fn test_task_packet_new() {
        let packet = TaskPacket::new("objective", "scope");
        assert_eq!(packet.objective, "objective");
        assert_eq!(packet.scope, "scope");
        assert!(packet.repo.is_empty());
        assert!(packet.branch_policy.is_empty());
        assert!(packet.acceptance_tests.is_empty());
        assert!(packet.commit_policy.is_empty());
        assert!(packet.reporting_contract.is_empty());
        assert!(packet.escalation_policy.is_empty());
    }

    #[test]
    fn test_task_packet_new_with_string_types() {
        let packet =
            TaskPacket::new(String::from("obj"), String::from("scope"));
        assert_eq!(packet.objective, "obj");
        assert_eq!(packet.scope, "scope");
    }

    // =====================================================================
    // TaskMessage tests
    // =====================================================================

    #[test]
    fn test_task_message_new() {
        let msg = TaskMessage::new("user", "hello", 12345);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.timestamp, 12345);
    }

    #[test]
    fn test_task_message_user() {
        let msg = TaskMessage::user("hello", 100);
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.timestamp, 100);
    }

    #[test]
    fn test_task_message_assistant() {
        let msg = TaskMessage::assistant("thinking", 200);
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "thinking");
        assert_eq!(msg.timestamp, 200);
    }

    #[test]
    fn test_task_message_with_string_args() {
        let msg = TaskMessage::new(
            String::from("system"),
            String::from("content"),
            300,
        );
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content, "content");
    }

    // =====================================================================
    // Task tests
    // =====================================================================

    #[test]
    fn test_task_new_sets_id_and_subject() {
        let task = Task::new("task-1", "Test Task");
        assert_eq!(task.id, "task-1");
        assert_eq!(task.subject, "Test Task");
    }

    #[test]
    fn test_task_new_sets_defaults() {
        let task = Task::new("t", "s");
        assert!(task.description.is_empty());
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.blocked_by.is_empty());
        assert!(task.blocks.is_empty());
        assert!(task.owner.is_empty());
        assert!(task.team_id.is_none());
        assert_eq!(task.priority, TaskPriority::Normal);
        assert!(task.task_packet.is_none());
        assert!(task.deadline.is_none());
        assert!(task.output.is_empty());
        assert!(task.messages.is_empty());
        assert!(task.created_at > 0);
        assert!(task.updated_at > 0);
    }

    #[test]
    fn test_task_with_description() {
        let task = Task::new("t", "s").with_description("desc");
        assert_eq!(task.description, "desc");
    }

    #[test]
    fn test_task_with_owner() {
        let task = Task::new("t", "s").with_owner("alice");
        assert_eq!(task.owner, "alice");
    }

    #[test]
    fn test_task_with_team() {
        let task = Task::new("t", "s").with_team("team-1");
        assert_eq!(task.team_id, Some("team-1".to_string()));
    }

    #[test]
    fn test_task_with_priority() {
        let task = Task::new("t", "s").with_priority(TaskPriority::Critical);
        assert_eq!(task.priority, TaskPriority::Critical);
    }

    #[test]
    fn test_task_with_status() {
        let task = Task::new("t", "s").with_status(TaskStatus::InProgress);
        assert_eq!(task.status, TaskStatus::InProgress);
    }

    #[test]
    fn test_task_with_deadline() {
        let task = Task::new("t", "s").with_deadline(999);
        assert_eq!(task.deadline, Some(999));
    }

    #[test]
    fn test_task_builder_chaining() {
        let task = Task::new("t", "s")
            .with_description("desc")
            .with_owner("bob")
            .with_team("team-2")
            .with_priority(TaskPriority::High)
            .with_status(TaskStatus::Completed)
            .with_deadline(500);

        assert_eq!(task.id, "t");
        assert_eq!(task.subject, "s");
        assert_eq!(task.description, "desc");
        assert_eq!(task.owner, "bob");
        assert_eq!(task.team_id, Some("team-2".to_string()));
        assert_eq!(task.priority, TaskPriority::High);
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.deadline, Some(500));
    }

    #[test]
    fn test_task_is_blocked() {
        let mut task = Task::new("t", "s");
        assert!(!task.is_blocked());

        task.blocked_by = vec!["dep-1".to_string()];
        assert!(task.is_blocked());

        task.blocked_by = vec![];
        assert!(!task.is_blocked());
    }

    #[test]
    fn test_task_is_assigned() {
        let mut task = Task::new("t", "s");
        assert!(!task.is_assigned());

        task.owner = "alice".to_string();
        assert!(task.is_assigned());

        task.owner = "".to_string();
        assert!(!task.is_assigned());
    }

    #[test]
    fn test_task_add_message() {
        let mut task = Task::new("t", "s");
        let initial_updated_at = task.updated_at;

        task.add_message("user", "hello");

        assert_eq!(task.messages.len(), 1);
        assert_eq!(task.messages[0].role, "user");
        assert_eq!(task.messages[0].content, "hello");
        assert!(task.updated_at >= initial_updated_at);
    }

    #[test]
    fn test_task_add_multiple_messages() {
        let mut task = Task::new("t", "s");
        task.add_message("user", "first");
        task.add_message("assistant", "second");
        task.add_message("user", "third");

        assert_eq!(task.messages.len(), 3);
        assert_eq!(task.messages[0].content, "first");
        assert_eq!(task.messages[1].content, "second");
        assert_eq!(task.messages[2].content, "third");
    }

    #[test]
    fn test_task_append_output() {
        let mut task = Task::new("t", "s");
        let initial_updated_at = task.updated_at;

        task.append_output("hello");
        assert_eq!(task.output, "hello");
        assert!(task.updated_at >= initial_updated_at);

        task.append_output(" world");
        assert_eq!(task.output, "hello world");
    }

    #[test]
    fn test_task_touch() {
        let mut task = Task::new("t", "s");
        let original_updated_at = task.updated_at;

        task.touch();
        assert!(task.updated_at >= original_updated_at);
    }

    // =====================================================================
    // TaskPatch tests
    // =====================================================================

    #[test]
    fn test_task_patch_new_is_empty() {
        let patch = TaskPatch::new();
        assert!(patch.subject.is_none());
        assert!(patch.description.is_none());
        assert!(patch.status.is_none());
        assert!(patch.blocked_by.is_none());
        assert!(patch.blocks.is_none());
        assert!(patch.owner.is_none());
        assert!(patch.team_id.is_none());
        assert!(patch.priority.is_none());
        assert!(patch.deadline.is_none());
        assert!(patch.output.is_none());
    }

    #[test]
    fn test_task_patch_with_status() {
        let patch = TaskPatch::new().with_status(TaskStatus::Completed);
        assert_eq!(patch.status, Some(TaskStatus::Completed));
    }

    #[test]
    fn test_task_patch_with_owner() {
        let patch = TaskPatch::new().with_owner("alice");
        assert_eq!(patch.owner, Some("alice".to_string()));
    }

    #[test]
    fn test_task_patch_with_team() {
        let patch = TaskPatch::new().with_team("team-1");
        assert_eq!(patch.team_id, Some("team-1".to_string()));
    }

    #[test]
    fn test_task_patch_chaining() {
        let patch = TaskPatch::new()
            .with_status(TaskStatus::InProgress)
            .with_owner("bob")
            .with_team("team-x");

        assert_eq!(patch.status, Some(TaskStatus::InProgress));
        assert_eq!(patch.owner, Some("bob".to_string()));
        assert_eq!(patch.team_id, Some("team-x".to_string()));
    }

    #[test]
    fn test_task_patch_default() {
        let patch = TaskPatch::default();
        assert!(patch.status.is_none());
        assert!(patch.owner.is_none());
    }

    // =====================================================================
    // TaskNode tests
    // =====================================================================

    #[test]
    fn test_task_node_new() {
        let node = TaskNode::new("task-1");
        assert_eq!(node.id, "task-1");
        assert!(node.blocked_by.is_empty());
        assert!(node.blocks.is_empty());
        assert_eq!(node.priority, TaskPriority::Normal);
        assert_eq!(node.status, TaskStatus::Pending);
    }

    #[test]
    fn test_task_node_with_priority() {
        let node = TaskNode::new("t").with_priority(TaskPriority::Critical);
        assert_eq!(node.priority, TaskPriority::Critical);
    }

    #[test]
    fn test_task_node_with_status() {
        let node = TaskNode::new("t").with_status(TaskStatus::Completed);
        assert_eq!(node.status, TaskStatus::Completed);
    }

    #[test]
    fn test_task_node_with_blocked_by() {
        let node = TaskNode::new("t")
            .with_blocked_by(vec!["dep1".to_string(), "dep2".to_string()]);
        assert_eq!(node.blocked_by, vec!["dep1", "dep2"]);
    }

    #[test]
    fn test_task_node_with_blocks() {
        let node = TaskNode::new("t").with_blocks(vec!["blocked1".to_string()]);
        assert_eq!(node.blocks, vec!["blocked1"]);
    }

    #[test]
    fn test_task_node_from_task() {
        let task = Task::new("t", "subject")
            .with_priority(TaskPriority::High)
            .with_status(TaskStatus::InProgress);
        let mut task = task;
        task.blocked_by = vec!["dep".to_string()];
        task.blocks = vec!["blocked".to_string()];

        let node = TaskNode::from(task);

        assert_eq!(node.id, "t");
        assert_eq!(node.priority, TaskPriority::High);
        assert_eq!(node.status, TaskStatus::InProgress);
        assert_eq!(node.blocked_by, vec!["dep"]);
        assert_eq!(node.blocks, vec!["blocked"]);
    }

    #[test]
    fn test_task_node_from_task_ref() {
        let mut task = Task::new("t", "subject");
        task.blocked_by = vec!["dep".to_string()];
        task.blocks = vec!["blocked".to_string()];

        let node = TaskNode::from(&task);

        assert_eq!(node.id, "t");
        assert_eq!(node.blocked_by, vec!["dep"]);
        assert_eq!(node.blocks, vec!["blocked"]);
    }

    // =====================================================================
    // TaskGraph basic tests
    // =====================================================================

    #[test]
    fn test_task_graph_new() {
        let graph = TaskGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
    }

    #[test]
    fn test_task_graph_default() {
        let graph = TaskGraph::default();
        assert!(graph.is_empty());
    }

    #[test]
    fn test_task_graph_add_task() {
        let mut graph = TaskGraph::new();
        let node = TaskNode::new("task-1");
        graph.add_task(node);

        assert!(!graph.is_empty());
        assert_eq!(graph.len(), 1);
        assert!(graph.get_task("task-1").is_some());
    }

    #[test]
    fn test_task_graph_add_task_from_struct() {
        let mut graph = TaskGraph::new();
        let task = Task::new("t", "subject");
        graph.add_task_from_struct(task);

        assert_eq!(graph.len(), 1);
        assert!(graph.get_task("t").is_some());
    }

    #[test]
    fn test_task_graph_remove_task() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("task-1"));

        let removed = graph.remove_task("task-1");
        assert!(removed.is_some());
        assert!(graph.is_empty());

        let removed_again = graph.remove_task("task-1");
        assert!(removed_again.is_none());
    }

    #[test]
    fn test_task_graph_task_ids() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));
        graph.add_task(TaskNode::new("c"));

        let ids: Vec<&String> = graph.task_ids();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_task_graph_clear() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));

        graph.clear();
        assert!(graph.is_empty());
    }

    // =====================================================================
    // TaskGraph dependency tests
    // =====================================================================

    #[test]
    fn test_task_graph_add_dependency() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));

        let result = graph.add_dependency("a", "b");
        assert!(result);

        let task_a = graph.get_task("a").unwrap();
        assert_eq!(task_a.blocks, vec!["b"]);

        let task_b = graph.get_task("b").unwrap();
        assert_eq!(task_b.blocked_by, vec!["a"]);
    }

    #[test]
    fn test_task_graph_add_dependency_nonexistent_tasks() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));

        // Try to add dependency with non-existent task
        assert!(!graph.add_dependency("a", "nonexistent"));
        assert!(!graph.add_dependency("nonexistent", "a"));
    }

    #[test]
    fn test_task_graph_add_dependency_idempotent() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));

        graph.add_dependency("a", "b");
        graph.add_dependency("a", "b"); // Add same dependency again

        let task_a = graph.get_task("a").unwrap();
        assert_eq!(task_a.blocks.len(), 1); // Should still be 1

        let task_b = graph.get_task("b").unwrap();
        assert_eq!(task_b.blocked_by.len(), 1);
    }

    #[test]
    fn test_task_graph_remove_dependency() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));
        graph.add_dependency("a", "b");

        graph.remove_dependency("a", "b");

        let task_a = graph.get_task("a").unwrap();
        assert!(task_a.blocks.is_empty());

        let task_b = graph.get_task("b").unwrap();
        assert!(task_b.blocked_by.is_empty());
    }

    #[test]
    fn test_task_graph_get_blocked_tasks() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));
        graph.add_task(TaskNode::new("c"));
        graph.add_dependency("a", "b");
        graph.add_dependency("a", "c");

        let blocked = graph.get_blocked_tasks("a");
        assert_eq!(blocked.len(), 2);
        assert!(blocked.contains(&"b".to_string()));
        assert!(blocked.contains(&"c".to_string()));
    }

    #[test]
    fn test_task_graph_get_blocking_tasks() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));
        graph.add_task(TaskNode::new("c"));
        graph.add_dependency("a", "c");
        graph.add_dependency("b", "c");

        let blocking = graph.get_blocking_tasks("c");
        assert_eq!(blocking.len(), 2);
        assert!(blocking.contains(&"a".to_string()));
        assert!(blocking.contains(&"b".to_string()));
    }

    // =====================================================================
    // TaskGraph check_dependencies tests
    // =====================================================================

    #[test]
    fn test_task_graph_check_dependencies_no_deps() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));

        // Task with no dependencies should be ready
        assert!(graph.check_dependencies("a"));
    }

    #[test]
    fn test_task_graph_check_dependencies_all_completed() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a").with_status(TaskStatus::Completed));
        graph.add_task(TaskNode::new("b").with_status(TaskStatus::Completed));
        graph.add_task(
            TaskNode::new("c")
                .with_blocked_by(vec!["a".to_string(), "b".to_string()]),
        );
        graph.add_dependency("a", "c");
        graph.add_dependency("b", "c");

        assert!(graph.check_dependencies("c"));
    }

    #[test]
    fn test_task_graph_check_dependencies_some_incomplete() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a").with_status(TaskStatus::Completed));
        graph.add_task(TaskNode::new("b").with_status(TaskStatus::Pending));
        graph.add_task(
            TaskNode::new("c")
                .with_blocked_by(vec!["a".to_string(), "b".to_string()]),
        );
        graph.add_dependency("a", "c");
        graph.add_dependency("b", "c");

        assert!(!graph.check_dependencies("c"));
    }

    #[test]
    fn test_task_graph_check_dependencies_nonexistent_task() {
        let graph = TaskGraph::new();
        assert!(!graph.check_dependencies("nonexistent"));
    }

    #[test]
    fn test_task_graph_check_dependencies_missing_dep_task() {
        let mut graph = TaskGraph::new();
        // Task depends on a task that doesn't exist in the graph
        graph.add_task(
            TaskNode::new("a").with_blocked_by(vec!["nonexistent".to_string()]),
        );

        // Missing dependency is treated as incomplete
        assert!(!graph.check_dependencies("a"));
    }

    // =====================================================================
    // TaskGraph get_ready_tasks tests
    // =====================================================================

    #[test]
    fn test_task_graph_get_ready_tasks_empty() {
        let graph = TaskGraph::new();
        assert!(graph.get_ready_tasks().is_empty());
    }

    #[test]
    fn test_task_graph_get_ready_tasks_no_deps() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));

        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 2);
    }

    #[test]
    fn test_task_graph_get_ready_tasks_with_deps() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));
        graph.add_task(
            TaskNode::new("c").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_dependency("a", "c");

        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 2); // a and b are ready
        assert!(ready.contains(&"a".to_string()));
        assert!(ready.contains(&"b".to_string()));
        assert!(!ready.contains(&"c".to_string()));
    }

    #[test]
    fn test_task_graph_get_ready_tasks_deps_completed() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a").with_status(TaskStatus::Completed));
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_dependency("a", "b");

        let ready = graph.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&"b".to_string()));
    }

    #[test]
    fn test_task_graph_get_ready_tasks_excludes_non_pending() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a").with_status(TaskStatus::InProgress));
        graph.add_task(TaskNode::new("b").with_status(TaskStatus::Completed));

        let ready = graph.get_ready_tasks();
        assert!(ready.is_empty());
    }

    #[test]
    fn test_task_graph_get_ready_tasks_by_priority() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("low").with_priority(TaskPriority::Low));
        graph.add_task(
            TaskNode::new("critical").with_priority(TaskPriority::Critical),
        );
        graph.add_task(TaskNode::new("high").with_priority(TaskPriority::High));
        graph.add_task(
            TaskNode::new("normal").with_priority(TaskPriority::Normal),
        );

        let ready = graph.get_ready_tasks_by_priority();
        assert_eq!(ready, vec!["critical", "high", "normal", "low"]);
    }

    // =====================================================================
    // TaskGraph topological_sort tests
    // =====================================================================

    #[test]
    fn test_task_graph_topological_sort_empty() {
        let graph = TaskGraph::new();
        let result = graph.topological_sort();
        assert_eq!(result, TopologicalSortResult::Sorted(vec![]));
    }

    #[test]
    fn test_task_graph_topological_sort_single_task() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));

        let result = graph.topological_sort();
        assert_eq!(
            result,
            TopologicalSortResult::Sorted(vec!["a".to_string()])
        );
    }

    #[test]
    fn test_task_graph_topological_sort_linear_chain() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("c").with_blocked_by(vec!["b".to_string()]),
        );
        graph.add_dependency("a", "b");
        graph.add_dependency("b", "c");

        let result = graph.topological_sort();
        assert_eq!(
            result,
            TopologicalSortResult::Sorted(vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string()
            ])
        );
    }

    #[test]
    fn test_task_graph_topological_sort_diamond() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("c").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("d")
                .with_blocked_by(vec!["b".to_string(), "c".to_string()]),
        );
        graph.add_dependency("a", "b");
        graph.add_dependency("a", "c");
        graph.add_dependency("b", "d");
        graph.add_dependency("c", "d");

        let result = graph.topological_sort();
        match result {
            TopologicalSortResult::Sorted(sorted) => {
                // a must come before b and c
                let pos_a = sorted.iter().position(|x| x == "a").unwrap();
                let pos_b = sorted.iter().position(|x| x == "b").unwrap();
                let pos_c = sorted.iter().position(|x| x == "c").unwrap();
                let pos_d = sorted.iter().position(|x| x == "d").unwrap();

                assert!(pos_a < pos_b);
                assert!(pos_a < pos_c);
                assert!(pos_b < pos_d);
                assert!(pos_c < pos_d);
            }
            TopologicalSortResult::CycleDetected(_) => {
                panic!("Expected sorted result, got cycle");
            }
        }
    }

    #[test]
    fn test_task_graph_topological_sort_independent_tasks() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));
        graph.add_task(TaskNode::new("c"));

        let result = graph.topological_sort();
        match result {
            TopologicalSortResult::Sorted(sorted) => {
                assert_eq!(sorted.len(), 3);
                assert!(sorted.contains(&"a".to_string()));
                assert!(sorted.contains(&"b".to_string()));
                assert!(sorted.contains(&"c".to_string()));
            }
            TopologicalSortResult::CycleDetected(_) => {
                panic!("Expected sorted result");
            }
        }
    }

    // =====================================================================
    // TaskGraph cycle detection tests
    // =====================================================================

    #[test]
    fn test_task_graph_cycle_detection_simple() {
        let mut graph = TaskGraph::new();
        graph.add_task(
            TaskNode::new("a").with_blocked_by(vec!["b".to_string()]),
        );
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_dependency("b", "a"); // a depends on b
        graph.add_dependency("a", "b"); // b depends on a

        assert!(graph.has_cycle());

        let result = graph.topological_sort();
        assert!(matches!(result, TopologicalSortResult::CycleDetected(_)));

        let cycles = graph.find_cycles();
        assert_eq!(cycles.len(), 2);
        assert!(cycles.contains(&"a".to_string()));
        assert!(cycles.contains(&"b".to_string()));
    }

    #[test]
    fn test_task_graph_cycle_detection_three_node_cycle() {
        let mut graph = TaskGraph::new();
        graph.add_task(
            TaskNode::new("a").with_blocked_by(vec!["c".to_string()]),
        );
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("c").with_blocked_by(vec!["b".to_string()]),
        );
        graph.add_dependency("c", "a");
        graph.add_dependency("a", "b");
        graph.add_dependency("b", "c");

        assert!(graph.has_cycle());

        let cycles = graph.find_cycles();
        assert_eq!(cycles.len(), 3);
    }

    #[test]
    fn test_task_graph_no_cycle() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_dependency("a", "b");

        assert!(!graph.has_cycle());
        assert!(graph.find_cycles().is_empty());
    }

    #[test]
    fn test_task_graph_cycle_with_non_cycle_nodes() {
        let mut graph = TaskGraph::new();
        // Independent node
        graph.add_task(TaskNode::new("independent"));
        // Cycle
        graph.add_task(
            TaskNode::new("a").with_blocked_by(vec!["b".to_string()]),
        );
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_dependency("b", "a");
        graph.add_dependency("a", "b");

        assert!(graph.has_cycle());

        let cycles = graph.find_cycles();
        assert_eq!(cycles.len(), 2);
        assert!(!cycles.contains(&"independent".to_string()));
    }

    // =====================================================================
    // TaskGraph update_status tests
    // =====================================================================

    #[test]
    fn test_task_graph_update_status() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));

        let result = graph.update_status("a", TaskStatus::Completed);
        assert!(result);

        let task = graph.get_task("a").unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn test_task_graph_update_status_nonexistent() {
        let mut graph = TaskGraph::new();
        let result = graph.update_status("nonexistent", TaskStatus::Completed);
        assert!(!result);
    }

    // =====================================================================
    // TaskGraph critical path tests
    // =====================================================================

    #[test]
    fn test_task_graph_critical_path_empty() {
        let graph = TaskGraph::new();
        assert!(graph.get_critical_path().is_empty());
    }

    #[test]
    fn test_task_graph_critical_path_single() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));

        let path = graph.get_critical_path();
        assert_eq!(path, vec!["a"]);
    }

    #[test]
    fn test_task_graph_critical_path_linear() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("c").with_blocked_by(vec!["b".to_string()]),
        );
        graph.add_dependency("a", "b");
        graph.add_dependency("b", "c");

        let path = graph.get_critical_path();
        assert_eq!(path, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_task_graph_critical_path_branching() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("c").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("d").with_blocked_by(vec!["b".to_string()]),
        );
        graph.add_dependency("a", "b");
        graph.add_dependency("a", "c");
        graph.add_dependency("b", "d");

        let path = graph.get_critical_path();
        // Critical path should be a -> b -> d (length 3)
        assert_eq!(path.len(), 3);
        assert!(path.contains(&"a".to_string()));
        assert!(path.contains(&"b".to_string()));
        assert!(path.contains(&"d".to_string()));
    }

    // =====================================================================
    // TaskGraph depth tests
    // =====================================================================

    #[test]
    fn test_task_graph_get_tasks_by_depth_empty() {
        let graph = TaskGraph::new();
        assert!(graph.get_tasks_by_depth().is_empty());
    }

    #[test]
    fn test_task_graph_get_tasks_by_depth_no_deps() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(TaskNode::new("b"));

        let depths = graph.get_tasks_by_depth();
        assert_eq!(depths.len(), 1);
        assert_eq!(depths[0].len(), 2);
    }

    #[test]
    fn test_task_graph_get_tasks_by_depth_linear() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("c").with_blocked_by(vec!["b".to_string()]),
        );
        graph.add_dependency("a", "b");
        graph.add_dependency("b", "c");

        let depths = graph.get_tasks_by_depth();
        assert_eq!(depths.len(), 3);
        assert!(depths[0].contains(&"a".to_string()));
        assert!(depths[1].contains(&"b".to_string()));
        assert!(depths[2].contains(&"c".to_string()));
    }

    #[test]
    fn test_task_graph_get_tasks_by_depth_diamond() {
        let mut graph = TaskGraph::new();
        graph.add_task(TaskNode::new("a"));
        graph.add_task(
            TaskNode::new("b").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("c").with_blocked_by(vec!["a".to_string()]),
        );
        graph.add_task(
            TaskNode::new("d")
                .with_blocked_by(vec!["b".to_string(), "c".to_string()]),
        );
        graph.add_dependency("a", "b");
        graph.add_dependency("a", "c");
        graph.add_dependency("b", "d");
        graph.add_dependency("c", "d");

        let depths = graph.get_tasks_by_depth();
        assert_eq!(depths.len(), 3);
        assert!(depths[0].contains(&"a".to_string()));
        assert!(depths[1].contains(&"b".to_string()));
        assert!(depths[1].contains(&"c".to_string()));
        assert!(depths[2].contains(&"d".to_string()));
    }

    // =====================================================================
    // TopologicalSortResult tests
    // =====================================================================

    #[test]
    fn test_topological_sort_result_eq() {
        let sorted1 = TopologicalSortResult::Sorted(vec!["a".to_string()]);
        let sorted2 = TopologicalSortResult::Sorted(vec!["a".to_string()]);
        let sorted3 = TopologicalSortResult::Sorted(vec!["b".to_string()]);
        let cycle1 =
            TopologicalSortResult::CycleDetected(vec!["a".to_string()]);
        let cycle2 =
            TopologicalSortResult::CycleDetected(vec!["a".to_string()]);

        assert_eq!(sorted1, sorted2);
        assert_ne!(sorted1, sorted3);
        assert_eq!(cycle1, cycle2);
        assert_ne!(sorted1, cycle1);
    }

    #[test]
    fn test_topological_sort_result_clone() {
        let sorted = TopologicalSortResult::Sorted(vec!["a".to_string()]);
        let cloned = sorted.clone();
        assert_eq!(sorted, cloned);
    }
}
