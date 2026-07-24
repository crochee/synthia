use serde::{Deserialize, Serialize};
use synthia_core::registry::RegistryItem;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderFilter {
    pub enabled_only: bool,
    pub provider_type: Option<String>,
}

impl ProviderFilter {
    pub fn accepts<E: RegistryItem>(&self, _item: &E) -> bool {
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub description: String,
}

impl RegistryItem for ProviderInfo {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }
}
