//! Tool scheduling types

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

/// Tool execution schedule with phases.
#[derive(Debug, Clone)]
pub struct ToolSchedule {
    pub phases: Vec<ExecutionPhase>,
    pub total_tools: usize,
}

impl ToolSchedule {
    /// Returns true if there are no phases.
    pub fn is_empty(&self) -> bool {
        self.phases.is_empty()
    }
}

/// A phase of tool execution with grouped tools.
#[derive(Debug, Clone)]
pub struct ExecutionPhase {
    pub phase_id: u32,
    pub tools: Vec<ToolCallInfo>,
    pub execution_mode: ExecutionMode,
}

/// Execution mode for a phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Parallel,
    Serial,
}

/// Information about a tool call for scheduling.
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub args: Value,
    pub is_read_only: bool,
    pub is_concurrency_safe: bool,
    pub depends_on: Vec<String>,
}

impl ToolCallInfo {
    /// Creates a new ToolCallInfo.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        args: Value,
        is_read_only: bool,
        is_concurrency_safe: bool,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            args,
            is_read_only,
            is_concurrency_safe,
            depends_on: Vec::new(),
        }
    }

    /// Creates a new ToolCallInfo with dependencies.
    pub fn with_dependencies(
        id: impl Into<String>,
        name: impl Into<String>,
        args: Value,
        is_read_only: bool,
        is_concurrency_safe: bool,
        depends_on: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            args,
            is_read_only,
            is_concurrency_safe,
            depends_on,
        }
    }
}

/// Error type for dependency graph operations
#[derive(Debug, Clone)]
pub struct DependencyError {
    pub message: String,
    pub cycle: Option<Vec<String>>,
}

impl std::fmt::Display for DependencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(cycle) = &self.cycle {
            write!(f, "Circular dependency detected: {}", cycle.join(" -> "))
        } else {
            write!(f, "Dependency error: {}", self.message)
        }
    }
}

impl std::error::Error for DependencyError {}

/// Tool dependency graph for scheduling
#[derive(Debug)]
pub struct DependencyGraph {
    nodes: HashMap<String, ToolCallInfo>,
    dependents: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    /// Creates a new empty dependency graph
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            dependents: HashMap::new(),
        }
    }

    /// Adds a tool node to the graph
    pub fn add_node(&mut self, tool: ToolCallInfo) {
        let id = tool.id.clone();
        self.nodes.insert(id.clone(), tool);
        self.dependents.entry(id).or_default();
    }

    /// Adds a dependency edge (from_id depends on to_id)
    pub fn add_dependency(&mut self, from_id: &str, to_id: &str) {
        self.dependents
            .entry(to_id.to_string())
            .or_default()
            .push(from_id.to_string());
    }

    /// Detects cycles using DFS
    fn detect_cycle(&self) -> Result<(), DependencyError> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for node_id in self.nodes.keys() {
            if visited.contains(node_id) {
                continue;
            }
            if let Some(cycle) = self.dfs_cycle_check(
                node_id,
                &mut visited,
                &mut rec_stack,
                &mut path,
            ) {
                return Err(DependencyError {
                    message: "Circular dependency detected".to_string(),
                    cycle: Some(cycle),
                });
            }
        }
        Ok(())
    }

    fn dfs_cycle_check(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());
        path.push(node_id.to_string());

        if let Some(dependents) = self.dependents.get(node_id) {
            for dependent in dependents {
                if !visited.contains(dependent) {
                    if let Some(cycle) = self
                        .dfs_cycle_check(dependent, visited, rec_stack, path)
                    {
                        return Some(cycle);
                    }
                } else if rec_stack.contains(dependent) {
                    let cycle_start = path
                        .iter()
                        .position(|x| x == dependent)
                        .unwrap_or_else(|| {
                            panic!("dependent {dependent} should be in path")
                        });
                    let mut cycle = path[cycle_start..].to_vec();
                    cycle.push(dependent.clone());
                    return Some(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node_id);
        None
    }

    /// Computes levels for each node using topological sort
    /// Level = max(dependent levels) + 1
    fn compute_levels(&self) -> HashMap<String, usize> {
        let mut in_degree = HashMap::new();
        let mut levels = HashMap::new();

        for id in self.nodes.keys() {
            in_degree.insert(id.clone(), 0);
        }

        for dependents in self.dependents.values() {
            for dependent in dependents {
                if let Some(degree) = in_degree.get_mut(dependent) {
                    *degree += 1;
                } else {
                    panic!("dependent {dependent} should exist");
                }
            }
        }

        let mut queue = VecDeque::new();
        for (id, degree) in &in_degree {
            if *degree == 0 {
                queue.push_back(id.clone());
                levels.insert(id.clone(), 0);
            }
        }

        while let Some(node_id) = queue.pop_front() {
            let current_level = levels[&node_id];

            if let Some(dependents) = self.dependents.get(&node_id) {
                for dependent in dependents {
                    let dependent_level =
                        levels.entry(dependent.clone()).or_insert(0);
                    *dependent_level =
                        (*dependent_level).max(current_level + 1);

                    if let Some(degree) = in_degree.get_mut(dependent) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent.clone());
                        }
                    } else {
                        panic!("dependent {dependent} should exist");
                    }
                }
            }
        }

        levels
    }

    /// Builds a ToolSchedule from the dependency graph
    pub fn build_schedule(self) -> Result<ToolSchedule, DependencyError> {
        if self.nodes.is_empty() {
            return Ok(ToolSchedule {
                phases: Vec::new(),
                total_tools: 0,
            });
        }

        self.detect_cycle()?;

        let levels = self.compute_levels();

        let max_level = levels.values().copied().max().unwrap_or(0);

        let mut level_groups: HashMap<usize, Vec<ToolCallInfo>> =
            HashMap::new();
        for (id, level) in &levels {
            if let Some(tool) = self.nodes.get(id) {
                level_groups.entry(*level).or_default().push(tool.clone());
            } else {
                panic!("level should correspond to existing node {id}");
            }
        }

        let mut phases = Vec::new();
        let mut phase_id = 0;

        for level in 0..=max_level {
            if let Some(mut tools) = level_groups.remove(&level) {
                let mut safe_tools = Vec::new();
                let mut unsafe_tools = Vec::new();

                for tool in tools.drain(..) {
                    if tool.is_concurrency_safe {
                        safe_tools.push(tool);
                    } else {
                        unsafe_tools.push(tool);
                    }
                }

                if !safe_tools.is_empty() {
                    phases.push(ExecutionPhase {
                        phase_id,
                        tools: safe_tools,
                        execution_mode: ExecutionMode::Parallel,
                    });
                    phase_id += 1;
                }

                for tool in unsafe_tools {
                    phases.push(ExecutionPhase {
                        phase_id,
                        tools: vec![tool],
                        execution_mode: ExecutionMode::Serial,
                    });
                    phase_id += 1;
                }
            }
        }

        let total_tools = phases.iter().map(|p| p.tools.len()).sum();

        Ok(ToolSchedule {
            phases,
            total_tools,
        })
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating tool schedules.
#[derive(Debug, Default)]
pub struct ScheduleBuilder {
    tools: Vec<ToolCallInfo>,
}

impl ScheduleBuilder {
    /// Creates a new schedule builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds tools to the schedule.
    pub fn with_tools(mut self, tools: Vec<ToolCallInfo>) -> Self {
        self.tools = tools;
        self
    }

    /// Builds the tool schedule using dependency graph.
    ///
    /// This method:
    /// 1. Builds a dependency graph from the tools
    /// 2. Detects circular dependencies
    /// 3. Computes execution levels using topological sort
    /// 4. Groups tools by level and concurrency safety
    /// 5. Creates phases: parallel for safe tools, serial for unsafe tools
    ///
    /// Tools with no dependencies run first.
    /// Tools at the same level can run in parallel (if concurrency-safe).
    /// Tools with dependencies run after their dependencies complete.
    pub fn build(self) -> ToolSchedule {
        if self.tools.is_empty() {
            return ToolSchedule {
                phases: Vec::new(),
                total_tools: 0,
            };
        }

        let mut graph = DependencyGraph::new();

        for tool in self.tools {
            let dependencies = tool.depends_on.clone();
            let tool_id = tool.id.clone();
            graph.add_node(tool);

            for depends_on_id in dependencies {
                graph.add_dependency(&tool_id, &depends_on_id);
            }
        }

        match graph.build_schedule() {
            Ok(schedule) => schedule,
            Err(e) => {
                tracing::error!("Failed to build tool schedule: {}", e);
                ToolSchedule {
                    phases: Vec::new(),
                    total_tools: 0,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn make_tool_info(
        id: &str,
        name: &str,
        read_only: bool,
        concurrency_safe: bool,
    ) -> ToolCallInfo {
        ToolCallInfo::new(id, name, json!({}), read_only, concurrency_safe)
    }

    #[test]
    fn test_schedule_builder_empty() {
        let schedule = ScheduleBuilder::new().build();
        assert!(schedule.is_empty());
        assert_eq!(schedule.total_tools, 0);
        assert!(schedule.phases.is_empty());
    }

    #[test]
    fn test_schedule_builder_single_safe_tool() {
        let tool = make_tool_info("1", "Read", true, true);
        let schedule = ScheduleBuilder::new().with_tools(vec![tool]).build();

        assert_eq!(schedule.total_tools, 1);
        assert_eq!(schedule.phases.len(), 1);
        assert_eq!(schedule.phases[0].execution_mode, ExecutionMode::Parallel);
        assert_eq!(schedule.phases[0].tools.len(), 1);
    }

    #[test]
    fn test_schedule_builder_single_unsafe_tool() {
        let tool = make_tool_info("1", "Write", false, false);
        let schedule = ScheduleBuilder::new().with_tools(vec![tool]).build();

        assert_eq!(schedule.total_tools, 1);
        assert_eq!(schedule.phases.len(), 1);
        assert_eq!(schedule.phases[0].execution_mode, ExecutionMode::Serial);
    }

    #[test]
    fn test_schedule_builder_mixed_tools_separated() {
        let tools = vec![
            make_tool_info("1", "Read1", true, true),
            make_tool_info("2", "Write", false, false),
            make_tool_info("3", "Read2", true, true),
        ];
        let schedule = ScheduleBuilder::new().with_tools(tools).build();

        assert_eq!(schedule.total_tools, 3);
        assert_eq!(schedule.phases.len(), 2);

        // First phase: safe tools in parallel
        assert_eq!(schedule.phases[0].execution_mode, ExecutionMode::Parallel);
        assert_eq!(schedule.phases[0].tools.len(), 2);

        // Second phase: unsafe tools serially
        assert_eq!(schedule.phases[1].execution_mode, ExecutionMode::Serial);
        assert_eq!(schedule.phases[1].tools.len(), 1);
    }

    #[test]
    fn test_schedule_builder_all_safe_parallel() {
        let tools = vec![
            make_tool_info("1", "Read1", true, true),
            make_tool_info("2", "Read2", true, true),
            make_tool_info("3", "Read3", true, true),
        ];
        let schedule = ScheduleBuilder::new().with_tools(tools).build();

        assert_eq!(schedule.total_tools, 3);
        assert_eq!(schedule.phases.len(), 1);
        assert_eq!(schedule.phases[0].execution_mode, ExecutionMode::Parallel);
        assert_eq!(schedule.phases[0].tools.len(), 3);
    }

    #[test]
    fn test_schedule_builder_all_unsafe_serial() {
        let tools = vec![
            make_tool_info("1", "Write1", false, false),
            make_tool_info("2", "Write2", false, false),
        ];
        let schedule = ScheduleBuilder::new().with_tools(tools).build();

        assert_eq!(schedule.total_tools, 2);
        // Each unsafe tool runs in its own phase
        assert_eq!(schedule.phases.len(), 2);
        assert_eq!(schedule.phases[0].execution_mode, ExecutionMode::Serial);
        assert_eq!(schedule.phases[0].tools.len(), 1);
        assert_eq!(schedule.phases[1].execution_mode, ExecutionMode::Serial);
        assert_eq!(schedule.phases[1].tools.len(), 1);
    }

    #[test]
    fn test_tool_call_info_new() {
        let info = ToolCallInfo::new(
            "id1",
            "Read",
            json!({"path": "/tmp"}),
            true,
            true,
        );
        assert_eq!(info.id, "id1");
        assert_eq!(info.name, "Read");
        assert_eq!(info.args, json!({"path": "/tmp"}));
        assert!(info.is_read_only);
        assert!(info.is_concurrency_safe);
    }

    #[test]
    fn test_tool_schedule_is_empty() {
        let empty = ToolSchedule {
            phases: Vec::new(),
            total_tools: 0,
        };
        assert!(empty.is_empty());

        let non_empty = ToolSchedule {
            phases: vec![ExecutionPhase {
                phase_id: 0,
                tools: vec![make_tool_info("1", "Read", true, true)],
                execution_mode: ExecutionMode::Parallel,
            }],
            total_tools: 1,
        };
        assert!(!non_empty.is_empty());
    }

    #[test]
    fn test_dependency_graph_empty() {
        let graph = DependencyGraph::new();
        let schedule = graph.build_schedule().unwrap();
        assert!(schedule.is_empty());
        assert_eq!(schedule.total_tools, 0);
    }

    #[test]
    fn test_dependency_graph_single_tool() {
        let mut graph = DependencyGraph::new();
        let tool = make_tool_info("1", "Read", true, true);
        graph.add_node(tool);

        let schedule = graph.build_schedule().unwrap();
        assert_eq!(schedule.total_tools, 1);
        assert_eq!(schedule.phases.len(), 1);
        assert_eq!(schedule.phases[0].execution_mode, ExecutionMode::Parallel);
    }

    #[test]
    fn test_dependency_graph_chain() {
        let mut graph = DependencyGraph::new();
        graph.add_node(make_tool_info("1", "Read1", true, true));
        graph.add_node(make_tool_info("2", "Read2", true, true));
        graph.add_node(make_tool_info("3", "Read3", true, true));

        graph.add_dependency("2", "1");
        graph.add_dependency("3", "2");

        let schedule = graph.build_schedule().unwrap();
        assert_eq!(schedule.total_tools, 3);
        assert_eq!(schedule.phases.len(), 3);
        assert_eq!(schedule.phases[0].tools[0].id, "1");
        assert_eq!(schedule.phases[1].tools[0].id, "2");
        assert_eq!(schedule.phases[2].tools[0].id, "3");
    }

    #[test]
    fn test_dependency_graph_parallel_at_same_level() {
        let mut graph = DependencyGraph::new();
        graph.add_node(make_tool_info("1", "Read1", true, true));
        graph.add_node(make_tool_info("2", "Read2", true, true));
        graph.add_node(make_tool_info("3", "Read3", true, true));

        graph.add_dependency("2", "1");
        graph.add_dependency("3", "1");

        let schedule = graph.build_schedule().unwrap();
        assert_eq!(schedule.total_tools, 3);
        assert_eq!(schedule.phases.len(), 2);
        assert_eq!(schedule.phases[0].tools.len(), 1);
        assert_eq!(schedule.phases[0].tools[0].id, "1");
        assert_eq!(schedule.phases[1].tools.len(), 2);
        assert_eq!(schedule.phases[1].execution_mode, ExecutionMode::Parallel);
    }

    #[test]
    fn test_dependency_graph_converging() {
        let mut graph = DependencyGraph::new();
        graph.add_node(make_tool_info("1", "Read1", true, true));
        graph.add_node(make_tool_info("2", "Read2", true, true));
        graph.add_node(make_tool_info("3", "Read3", true, true));

        graph.add_dependency("3", "1");
        graph.add_dependency("3", "2");

        let schedule = graph.build_schedule().unwrap();
        assert_eq!(schedule.total_tools, 3);
        assert_eq!(schedule.phases.len(), 2);
        assert_eq!(schedule.phases[0].tools.len(), 2);
        assert_eq!(schedule.phases[0].execution_mode, ExecutionMode::Parallel);
        assert_eq!(schedule.phases[1].tools.len(), 1);
        assert_eq!(schedule.phases[1].tools[0].id, "3");
    }

    #[test]
    fn test_dependency_graph_circular_detection() {
        let mut graph = DependencyGraph::new();
        graph.add_node(make_tool_info("1", "Tool1", true, true));
        graph.add_node(make_tool_info("2", "Tool2", true, true));
        graph.add_node(make_tool_info("3", "Tool3", true, true));

        graph.add_dependency("2", "1");
        graph.add_dependency("3", "2");
        graph.add_dependency("1", "3");

        let result = graph.build_schedule();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.cycle.is_some());
    }

    #[test]
    fn test_dependency_graph_mixed_safety() {
        let mut graph = DependencyGraph::new();
        graph.add_node(make_tool_info("1", "Read1", true, true));
        graph.add_node(make_tool_info("2", "Write1", false, false));
        graph.add_node(make_tool_info("3", "Read2", true, true));

        graph.add_dependency("2", "1");
        graph.add_dependency("3", "1");

        let schedule = graph.build_schedule().unwrap();
        assert_eq!(schedule.total_tools, 3);

        // Phase 0: Tool 1 (safe, no dependencies, level 0)
        assert_eq!(schedule.phases[0].tools.len(), 1);
        assert_eq!(schedule.phases[0].tools[0].id, "1");
        assert_eq!(schedule.phases[0].execution_mode, ExecutionMode::Parallel);

        // Phase 1: Tool 3 (safe, depends on 1, level 1) - safe tools first
        assert_eq!(schedule.phases[1].tools.len(), 1);
        assert_eq!(schedule.phases[1].tools[0].id, "3");
        assert_eq!(schedule.phases[1].execution_mode, ExecutionMode::Parallel);

        // Phase 2: Tool 2 (unsafe, depends on 1, level 1) - unsafe tools after
        assert_eq!(schedule.phases[2].tools.len(), 1);
        assert_eq!(schedule.phases[2].tools[0].id, "2");
        assert_eq!(schedule.phases[2].execution_mode, ExecutionMode::Serial);
    }

    #[test]
    fn test_schedule_builder_with_dependencies() {
        let tools = vec![
            ToolCallInfo::with_dependencies(
                "1",
                "Read1",
                serde_json::json!({}),
                true,
                true,
                vec![],
            ),
            ToolCallInfo::with_dependencies(
                "2",
                "Read2",
                serde_json::json!({}),
                true,
                true,
                vec!["1".to_string()],
            ),
            ToolCallInfo::with_dependencies(
                "3",
                "Read3",
                serde_json::json!({}),
                true,
                true,
                vec!["1".to_string()],
            ),
        ];

        let schedule = ScheduleBuilder::new().with_tools(tools).build();
        assert_eq!(schedule.total_tools, 3);
        assert_eq!(schedule.phases.len(), 2);
        assert_eq!(schedule.phases[0].tools.len(), 1);
        assert_eq!(schedule.phases[1].tools.len(), 2);
    }
}
