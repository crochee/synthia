use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolFilter {
    pub name_prefix: Option<String>,
}

impl ToolFilter {
    pub fn accepts<E: synthia_core::registry::RegistryItem>(
        &self,
        item: &E,
    ) -> bool {
        if let Some(ref prefix) = self.name_prefix
            && !item.name().starts_with(prefix)
        {
            return false;
        }
        true
    }
}
