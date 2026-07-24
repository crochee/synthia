use std::{path::PathBuf, sync::Arc};

use notify::{
    Event,
    RecommendedWatcher,
    RecursiveMode,
    Watcher as NotifyWatcher,
};
use parking_lot::RwLock;
use synthia_core::Error;

use super::event_handler::handle_event;
use crate::{loader::SkillLoader, registry::SkillRegistry};

pub struct SkillWatcher {
    watcher: Option<RecommendedWatcher>,
    registry: Arc<RwLock<SkillRegistry>>,
    loader: Arc<SkillLoader>,
    skills_dir: PathBuf,
}

impl SkillWatcher {
    pub fn new(
        skills_dir: PathBuf,
        registry: Arc<RwLock<SkillRegistry>>,
        loader: Arc<SkillLoader>,
    ) -> Result<Self, Error> {
        if !skills_dir.exists() {
            return Err(Error::NotFound(format!(
                "skills directory: {}",
                skills_dir.display()
            )));
        }

        Ok(Self {
            watcher: None,
            registry,
            loader,
            skills_dir,
        })
    }

    pub fn start(&mut self) -> Result<(), Error> {
        let skills_dir = self.skills_dir.clone();
        let registry = Arc::clone(&self.registry);
        let loader = Arc::clone(&self.loader);

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    handle_event(&event, &registry, &loader);
                }
            },
            notify::Config::default(),
        )
        .map_err(|e| Error::Internal(format!("notify error: {}", e)))?;

        watcher
            .watch(&skills_dir, RecursiveMode::Recursive)
            .map_err(|e| Error::Internal(format!("notify error: {}", e)))?;

        tracing::info!(dir = ?skills_dir, "Skill watcher started");
        self.watcher = Some(watcher);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
            tracing::info!(dir = ?self.skills_dir, "Skill watcher stopped");
        }
        Ok(())
    }
}
