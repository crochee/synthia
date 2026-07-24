use std::collections::{HashMap, HashSet, VecDeque};

use synthia_core::Error;

#[derive(Debug, Clone, Default)]
pub struct Topology {
    dependencies: HashMap<String, HashSet<String>>,
    dependents: HashMap<String, HashSet<String>>,
}

impl Topology {
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
            dependents: HashMap::new(),
        }
    }

    pub fn add_dependency(
        &mut self,
        from: String,
        to: String,
    ) -> Result<(), Error> {
        if self.would_create_cycle(&from, &to) {
            return Err(Error::Validation(format!(
                "adding dependency {from} -> {to} would create a cycle"
            )));
        }
        self.dependencies
            .entry(to.clone())
            .or_default()
            .insert(from.clone());
        self.dependents.entry(from).or_default().insert(to);
        Ok(())
    }

    pub fn remove_dependency(&mut self, from: &str, to: &str) {
        if let Some(deps) = self.dependencies.get_mut(to) {
            deps.remove(from);
            if deps.is_empty() {
                self.dependencies.remove(to);
            }
        }
        if let Some(deps) = self.dependents.get_mut(from) {
            deps.remove(to);
            if deps.is_empty() {
                self.dependents.remove(from);
            }
        }
    }

    pub fn topological_sort(&self) -> Result<Vec<String>, Error> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut all_nodes: HashSet<&str> = HashSet::new();

        for node in self.dependencies.keys() {
            all_nodes.insert(node.as_str());
        }
        for node in self.dependents.keys() {
            all_nodes.insert(node.as_str());
        }

        for node in &all_nodes {
            in_degree.entry(node).or_insert(0);
        }

        for deps in self.dependencies.values() {
            for dep in deps {
                *in_degree.entry(dep.as_str()).or_insert(0) += 0;
            }
        }

        for (node, deps) in &self.dependencies {
            *in_degree.entry(node.as_str()).or_insert(0) = deps.len();
        }

        let mut queue: VecDeque<&str> = VecDeque::new();
        for (&node, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(node);
            }
        }

        let mut result: Vec<String> = Vec::new();
        while let Some(node) = queue.pop_front() {
            result.push(node.to_string());
            if let Some(deps) = self.dependents.get(node) {
                for dependent in deps {
                    if let Some(degree) = in_degree.get_mut(dependent.as_str())
                    {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent.as_str());
                        }
                    }
                }
            }
        }

        if result.len() != all_nodes.len() {
            return Err(Error::Validation(
                "dependency cycle detected".to_string(),
            ));
        }

        Ok(result)
    }

    pub fn detect_cycle(&self) -> Option<Vec<String>> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut rec_stack: HashSet<String> = HashSet::new();
        let mut path: Vec<String> = Vec::new();

        let all_nodes: HashSet<String> = self
            .dependencies
            .keys()
            .chain(self.dependents.keys())
            .cloned()
            .collect();

        for node in all_nodes {
            if self.dfs_cycle(&node, &mut visited, &mut rec_stack, &mut path) {
                return Some(path);
            }
        }

        None
    }

    fn dfs_cycle(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        if rec_stack.contains(node) {
            path.push(node.to_string());
            return true;
        }
        if visited.contains(node) {
            return false;
        }

        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(deps) = self.dependents.get(node) {
            for dep in deps {
                if self.dfs_cycle(dep, visited, rec_stack, path) {
                    return true;
                }
            }
        }

        rec_stack.remove(node);
        path.pop();
        false
    }

    pub fn get_dependencies(&self, id: &str) -> Vec<String> {
        self.dependencies
            .get(id)
            .map(|deps| deps.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_dependents(&self, id: &str) -> Vec<String> {
        self.dependents
            .get(id)
            .map(|deps| deps.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn remove_node(&mut self, id: &str) {
        if let Some(deps) = self.dependencies.remove(id) {
            for dep in deps {
                if let Some(dependents) = self.dependents.get_mut(&dep) {
                    dependents.remove(id);
                    if dependents.is_empty() {
                        self.dependents.remove(&dep);
                    }
                }
            }
        }
        if let Some(dependents) = self.dependents.remove(id) {
            for dep in dependents {
                if let Some(deps) = self.dependencies.get_mut(&dep) {
                    deps.remove(id);
                    if deps.is_empty() {
                        self.dependencies.remove(&dep);
                    }
                }
            }
        }
    }

    fn would_create_cycle(&self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(from);
        while let Some(node) = queue.pop_front() {
            if node == to {
                return true;
            }
            if visited.insert(node)
                && let Some(deps) = self.dependencies.get(node)
            {
                for dep in deps {
                    queue.push_back(dep.as_str());
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_dependency() {
        let mut topo = Topology::new();
        topo.add_dependency("a".to_string(), "b".to_string())
            .unwrap();
        assert_eq!(topo.get_dependencies("b"), vec!["a"]);
        assert_eq!(topo.get_dependents("a"), vec!["b"]);
    }

    #[test]
    fn test_remove_dependency() {
        let mut topo = Topology::new();
        topo.add_dependency("a".to_string(), "b".to_string())
            .unwrap();
        topo.remove_dependency("a", "b");
        assert!(topo.get_dependencies("b").is_empty());
        assert!(topo.get_dependents("a").is_empty());
    }

    #[test]
    fn test_topological_sort() {
        let mut topo = Topology::new();
        topo.add_dependency("a".to_string(), "b".to_string())
            .unwrap();
        topo.add_dependency("b".to_string(), "c".to_string())
            .unwrap();
        let sorted = topo.topological_sort().unwrap();
        let pos_a = sorted.iter().position(|x| x == "a").unwrap();
        let pos_b = sorted.iter().position(|x| x == "b").unwrap();
        let pos_c = sorted.iter().position(|x| x == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_detect_cycle_no_cycle() {
        let mut topo = Topology::new();
        topo.add_dependency("a".to_string(), "b".to_string())
            .unwrap();
        assert!(topo.detect_cycle().is_none());
    }

    #[test]
    fn test_detect_cycle_with_cycle() {
        let mut topo = Topology::new();
        topo.dependencies
            .entry("b".to_string())
            .or_default()
            .insert("a".to_string());
        topo.dependents
            .entry("a".to_string())
            .or_default()
            .insert("b".to_string());
        topo.dependencies
            .entry("a".to_string())
            .or_default()
            .insert("b".to_string());
        topo.dependents
            .entry("b".to_string())
            .or_default()
            .insert("a".to_string());
        assert!(topo.detect_cycle().is_some());
    }

    #[test]
    fn test_prevent_cycle_on_add() {
        let mut topo = Topology::new();
        topo.add_dependency("a".to_string(), "b".to_string())
            .unwrap();
        topo.add_dependency("b".to_string(), "c".to_string())
            .unwrap();
        let result = topo.add_dependency("c".to_string(), "a".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_self_cycle_prevented() {
        let mut topo = Topology::new();
        let result = topo.add_dependency("a".to_string(), "a".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_node() {
        let mut topo = Topology::new();
        topo.add_dependency("a".to_string(), "b".to_string())
            .unwrap();
        topo.add_dependency("b".to_string(), "c".to_string())
            .unwrap();
        topo.remove_node("b");
        assert!(topo.get_dependencies("b").is_empty());
        assert!(topo.get_dependents("a").is_empty());
        assert!(topo.get_dependencies("c").is_empty());
    }

    #[test]
    fn test_get_dependencies_empty() {
        let topo = Topology::new();
        assert!(topo.get_dependencies("nonexistent").is_empty());
    }

    #[test]
    fn test_get_dependents_empty() {
        let topo = Topology::new();
        assert!(topo.get_dependents("nonexistent").is_empty());
    }
}
