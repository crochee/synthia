//! PBAC Policy module — defines policies, conditions, and
//! combining algorithms.
//!
//! # Module Layout
//!
//! - [`condition`]: [`condition::Condition`] struct +
//!   [`condition::ConditionResult`] enum + their impls. The
//!   surface used by [`super::context::AccessRequest`]
//!   evaluation.
//! - [`condition_definition`]\: [`condition_definition::ConditionDefinition`]
//!   tagged enum (10 variants) + its large `evaluate` method
//!   that dispatches on the variant tag.
//! - [`resolve`]: Free function [`resolve::resolve_attribute`]
//!   — translates a dotted `subject.id` / `resource.name` /
//!   `environment.risk_score` path into a
//!   `serde_json::Value`. Used by `AttributeEquals` and
//!   `AttributeCompare` variants.
//! - [`policy_trait`]: [`policy_trait::Policy`] +
//!   [`policy_trait::AsyncPolicy`] traits +
//!   [`policy_trait::PolicyResult`] enum.
//! - [`policy_set`]: [`policy_set::PolicySet`] struct + its
//!   `evaluate` (4 combining algorithms) +
//!   [`policy_set::CombiningAlgorithm`] enum.
//! - [`policy_definition`]: The data model
//!   [`policy_definition::PolicyDefinition`] +
//!   [`policy_definition::PolicyEffect`] +
//!   [`policy_definition::PolicyTarget`] +
//!   [`policy_definition::SubjectTarget`] +
//!   [`policy_definition::ResourceTarget`].
//! - [`tests`]: 5 unit tests covering `RoleCheck`,
//!   `RiskThreshold`, `ConditionResult` accessors,
//!   `resolve_attribute`, and `RoleCheck` with
//!   `require_all=true`.

mod condition;
mod condition_definition;
mod policy_definition;
mod policy_set;
mod policy_trait;
mod resolve;

#[cfg(test)]
mod tests;

pub use condition::{Condition, ConditionResult};
pub use condition_definition::ConditionDefinition;
pub use policy_definition::{
    PolicyDefinition,
    PolicyEffect,
    PolicyTarget,
    ResourceTarget,
    SubjectTarget,
};
pub use policy_set::{CombiningAlgorithm, PolicySet};
pub use policy_trait::{AsyncPolicy, Policy, PolicyResult};
pub use resolve::resolve_attribute;
