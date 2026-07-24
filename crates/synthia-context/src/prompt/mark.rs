//! Cache control mark for the prompt snapshot.
//!
//! The concrete types live in the shared `synthia-cache-mark` crate so that
//! both the context layer and the provider layer reference a single
//! definition. This module simply re-exports them to preserve the existing
//! `crate::prompt::mark::CacheControlMark` path.

pub use synthia_cache_mark::{CacheControlMark, CacheScope, CacheTtl};
