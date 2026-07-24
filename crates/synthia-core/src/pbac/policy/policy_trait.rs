//! The [`Policy`] + [`AsyncPolicy`] traits +
//! [`PolicyResult`] enum.
//!
//! A `Policy` is any object that can be asked "does this
//! `AccessRequest` match?". The `AsyncPolicy` trait allows
//! policies that need to call out to a remote service (e.g. a
//! delegated-decision authority).

use super::{super::context::AccessRequest, condition::Condition};

pub trait Policy: Send + Sync {
    fn name(&self) -> &str;
    fn matches(&self, request: &AccessRequest) -> PolicyResult;
    fn conditions(&self) -> Option<Vec<Condition>>;
    fn priority(&self) -> i32;
}

pub trait AsyncPolicy: Policy {
    fn matches_async(
        &self,
        request: &AccessRequest,
    ) -> impl std::future::Future<Output = PolicyResult> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyResult {
    Match,
    NoMatch,
    Indeterminate(String),
}
