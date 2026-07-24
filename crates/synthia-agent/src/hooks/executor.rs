//! The [`HookExecutor`] struct itself, plus its `new` / `is_empty` /
//! `Default` impls. Method bodies live in [`super::lifecycle`] (the
//! six `fire_*` methods) and [`super::domain`] (the three `on_*`
//! methods).

use synthia_hook::HookRegistry;

#[derive(Default)]
pub struct HookExecutor {
    pub registry: HookRegistry,
}

impl HookExecutor {
    pub fn new(registry: HookRegistry) -> Self {
        Self { registry }
    }

    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }
}
