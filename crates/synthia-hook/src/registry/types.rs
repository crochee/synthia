//! The 3 public data records carried by the hook registry:
//!
//! - [`HookHandle`] — the registered hook's ULID, returned
//!   from [`super::registry::HookRegistry::register_hook`]
//!   and used by
//!   [`super::registry::HookRegistry::unregister_by_handle`].
//! - [`HookFilter`] — the `Registry<HookInfo>::Filter` type
//!   (a single-field filter on the hook's `name` substring).
//! - [`HookInfo`] — the metadata record that backs the
//!   `Registry<HookInfo>` trait impl.

use serde::{Deserialize, Serialize};
use synthia_core::registry::RegistryItem;
use ulid::Ulid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HookHandle(pub Ulid);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookFilter {
    pub hook_type: Option<String>,
}

impl HookFilter {
    pub fn accepts(&self, item: &HookInfo) -> bool {
        if let Some(ref hook_type) = self.hook_type
            && !item.name.contains(hook_type)
        {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}

impl RegistryItem for HookInfo {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}
