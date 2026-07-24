//! `PromptSection` trait + blanket `Box<T>` impl.
//!
//! The trait is the unified contract every concrete section in
//! [`super`] implements. Three pieces of metadata per section:
//!
//! - [`PromptSection::name`] — stable identifier used for
//!   `tracing`/log tags and KV-cache prefix hashing.
//! - [`PromptSection::caching`] — whether the rendered output
//!   is [`SectionCaching::Cached`] (identical across calls),
//!   [`SectionCaching::SessionCached`] (identical within a
//!   session), or [`SectionCaching::Volatile`] (may change every
//!   call). Drives the prefix-stability tracking in
//!   `crate::prompt::builder`.
//! - [`PromptSection::build`] — render the section's text given
//!   the call's [`PromptContext`].
//!
//! The blanket `impl<T: PromptSection + ?Sized> PromptSection
//! for Box<T>` lets the prompt builder hold a
//! `Vec<Box<dyn PromptSection>>` without forcing every concrete
//! section to manually re-implement the trait methods.

use anyhow::Result;

use crate::prompt::{PromptContext, SectionCaching};

pub trait PromptSection: Send + Sync {
    fn name(&self) -> &str;
    fn caching(&self) -> SectionCaching;
    fn build(&self, ctx: &PromptContext<'_>) -> Result<String>;
}

impl<T: PromptSection + ?Sized> PromptSection for Box<T> {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn caching(&self) -> SectionCaching {
        (**self).caching()
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        (**self).build(ctx)
    }
}
