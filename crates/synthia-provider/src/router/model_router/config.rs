use synthia_core::Error;

use super::{super::config::RoutingConfig, ModelRouter};

#[derive(Debug, serde::Deserialize)]
struct TomlConfig {
    fallback_chain: Option<std::collections::HashMap<String, Vec<String>>>,
}

impl ModelRouter {
    pub fn load_routing_config(
        &mut self,
        toml_content: &str,
    ) -> Result<(), Error> {
        let config = RoutingConfig::from_toml(toml_content).map_err(|e| {
            Error::Config(format!("routing config error: {}", e))
        })?;
        self.routing_config = config;
        Ok(())
    }

    pub fn reload_config(&mut self, new_config: RoutingConfig) {
        self.routing_config = new_config;
    }

    pub fn load_fallback_chain_from_toml(
        &mut self,
        config_path: &str,
    ) -> Result<(), Error> {
        let content = std::fs::read_to_string(config_path).map_err(|e| {
            Error::Config(format!("Failed to read config: {}", e))
        })?;
        let toml_config: TomlConfig =
            toml::from_str(&content).map_err(|e| {
                Error::Config(format!("Failed to parse TOML: {}", e))
            })?;

        if let Some(chains) = toml_config.fallback_chain {
            for (primary, fallbacks) in chains {
                self.fallback_chains.insert(primary, fallbacks);
            }
        }
        Ok(())
    }
}
