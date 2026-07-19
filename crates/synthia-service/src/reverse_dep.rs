//! Reverse-dependency tracking (no broker).
//!
//! PR-3.3 tracks which services depend on which via a directed acyclic
//! graph stored in a `DashMap<ServiceId, BTreeSet<ServiceId>>`. Cycle
//! detection on bind prevents circular dependencies.
//!
//! See `openspec/.../specs/service-registry-completion/spec.md`
//! (Requirement: "reverse-dependency resolution (no broker)").

use std::collections::BTreeSet;

use dashmap::DashMap;

use crate::output_bound::ServiceRegistryError;

/// Stable identifier for a registered service.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServiceId(pub String);

impl std::fmt::Display for ServiceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Thread-safe reverse-dependency graph.
///
/// Edges flow **from dependent → dependency**: `A → B` means "A depends on B".
/// Reverse lookup (`reverse_dependents_of`) returns all services that depend
/// on a given service.
pub(crate) struct ReverseDepGraph {
    /// Forward edges: `ServiceId` → set of services it depends on.
    forward: DashMap<ServiceId, BTreeSet<ServiceId>>,
    /// Reverse edges: `ServiceId` → set of services that depend on it.
    reverse: DashMap<ServiceId, BTreeSet<ServiceId>>,
}

impl ReverseDepGraph {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self {
            forward: DashMap::new(),
            reverse: DashMap::new(),
        }
    }

    /// Record that `dependent` depends on `dependency`.
    ///
    /// Performs cycle detection before insertion. If adding this edge
    /// would create a cycle, returns `Err(ServiceRegistryError::Cycle)`.
    pub fn add_edge(
        &self,
        dependent: ServiceId,
        dependency: ServiceId,
    ) -> Result<(), ServiceRegistryError> {
        // Self-dependency is a trivial cycle.
        if dependent == dependency {
            return Err(ServiceRegistryError::Cycle {
                path: format!("{dependent} → {dependent}"),
            });
        }

        // Check if adding dependent → dependency creates a cycle by
        // verifying that `dependency` cannot reach `dependent` via
        // existing forward edges (DFS from dependency).
        if self.reachable(&dependency, &dependent) {
            return Err(ServiceRegistryError::Cycle {
                path: format!("{dependent} → {dependency} → ... → {dependent}"),
            });
        }

        // Insert forward edge: dependent → dependency.
        self.forward
            .entry(dependent.clone())
            .or_default()
            .insert(dependency.clone());

        // Insert reverse edge: dependency → dependent.
        self.reverse
            .entry(dependency)
            .or_default()
            .insert(dependent);

        Ok(())
    }

    /// Remove a service and all its edges from the graph.
    pub fn remove_service(&self, id: &ServiceId) {
        // Remove from forward edges.
        if let Some((_, deps)) = self.forward.remove(id) {
            for dep in &deps {
                if let Some(mut rdeps) = self.reverse.get_mut(dep) {
                    rdeps.remove(id);
                }
            }
        }

        // Remove from reverse edges (this service is a dependency of others).
        if let Some((_, dependents)) = self.reverse.remove(id) {
            for dep_of in &dependents {
                if let Some(mut fwd) = self.forward.get_mut(dep_of) {
                    fwd.remove(id);
                }
            }
        }
    }

    /// Return all services that depend on `id` (reverse lookup).
    ///
    /// Returns an empty set if `id` has no dependents.
    pub fn reverse_dependents_of(&self, id: &ServiceId) -> BTreeSet<ServiceId> {
        self.reverse
            .get(id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// Check if `target` is reachable from `source` via forward edges (DFS).
    fn reachable(&self, source: &ServiceId, target: &ServiceId) -> bool {
        let mut visited = BTreeSet::new();
        let mut stack = vec![source.clone()];

        while let Some(current) = stack.pop() {
            if current == *target {
                return true;
            }
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            if let Some(deps) = self.forward.get(&current) {
                for dep in deps.value() {
                    if !visited.contains(dep) {
                        stack.push(dep.clone());
                    }
                }
            }
        }

        false
    }
}

impl Default for ReverseDepGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_edge_and_reverse_lookup() {
        let graph = ReverseDepGraph::new();
        let a = ServiceId("a".into());
        let b = ServiceId("b".into());

        graph.add_edge(a.clone(), b.clone()).unwrap();

        // b has a as a reverse dependent (i.e., a depends on b)
        let rdeps = graph.reverse_dependents_of(&b);
        assert!(rdeps.contains(&a));
    }

    #[test]
    fn self_dependency_rejected() {
        let graph = ReverseDepGraph::new();
        let a = ServiceId("a".into());

        let result = graph.add_edge(a.clone(), a);
        assert!(matches!(result, Err(ServiceRegistryError::Cycle { .. })));
    }

    #[test]
    fn cycle_detection_on_bind() {
        let graph = ReverseDepGraph::new();
        let a = ServiceId("a".into());
        let b = ServiceId("b".into());
        let c = ServiceId("c".into());

        // a → b → c is fine
        graph.add_edge(a.clone(), b.clone()).unwrap();
        graph.add_edge(b.clone(), c.clone()).unwrap();

        // c → a would create a cycle
        let result = graph.add_edge(c.clone(), a.clone());
        assert!(matches!(result, Err(ServiceRegistryError::Cycle { .. })));
    }

    #[test]
    fn remove_service_cleans_edges() {
        let graph = ReverseDepGraph::new();
        let a = ServiceId("a".into());
        let b = ServiceId("b".into());

        graph.add_edge(a.clone(), b.clone()).unwrap();
        graph.remove_service(&a);

        // a's removal should clear reverse edge from b
        assert!(graph.reverse_dependents_of(&b).is_empty());
    }

    #[test]
    fn remove_dependency_cleans_reverse() {
        let graph = ReverseDepGraph::new();
        let a = ServiceId("a".into());
        let b = ServiceId("b".into());

        graph.add_edge(a.clone(), b.clone()).unwrap();
        graph.remove_service(&b);

        // b's removal should clear forward edge from a (reverse lookup of b)
        assert!(graph.reverse_dependents_of(&b).is_empty());
    }

    #[test]
    fn reverse_dependents_of_unknown_returns_empty() {
        let graph = ReverseDepGraph::new();
        let unknown = ServiceId("unknown".into());
        assert!(graph.reverse_dependents_of(&unknown).is_empty());
    }

    #[test]
    fn longer_cycle_detected() {
        let graph = ReverseDepGraph::new();
        let a = ServiceId("a".into());
        let b = ServiceId("b".into());
        let c = ServiceId("c".into());
        let d = ServiceId("d".into());

        graph.add_edge(a.clone(), b.clone()).unwrap();
        graph.add_edge(b.clone(), c.clone()).unwrap();
        graph.add_edge(c.clone(), d.clone()).unwrap();

        // d → a would close the cycle a → b → c → d → a
        let result = graph.add_edge(d.clone(), a);
        assert!(matches!(result, Err(ServiceRegistryError::Cycle { .. })));
    }
}
