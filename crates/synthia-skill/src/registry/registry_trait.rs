//! `impl Registry<Skill> for SkillRegistry`.
//!
//! Trait impl is isolated so the canonical `Registry` contract surface
//! is discoverable in one place, separate from the inherent
//! SkillRegistry impls in [`super::lifecycle`] and [`super::query`].

use async_trait::async_trait;
use synthia_core::{Error, registry::Registry};

use super::types::{SkillFilter, SkillRegistry};

#[async_trait]
impl Registry<crate::types::Skill> for SkillRegistry {
    type Filter = SkillFilter;

    async fn register(
        &self,
        item: crate::types::Skill,
    ) -> Result<crate::types::Skill, Error> {
        let name = item.metadata.name.clone();
        let mut skills = self.skills.write();
        if skills.contains_key(&name) {
            return Err(Error::AlreadyExists(name));
        }
        skills.insert(name.clone(), item.clone());
        drop(skills);
        self.rebuild_bm25_index_internal();
        self.rebuild_vector_index_internal();
        Ok(item)
    }

    async fn unregister(&self, name: &str) -> Result<(), Error> {
        if self.unregister(name) {
            Ok(())
        } else {
            Err(Error::NotFound(name.to_string()))
        }
    }

    async fn get(
        &self,
        name: &str,
    ) -> Result<Option<crate::types::Skill>, Error> {
        Ok(self.skills.read().get(name).cloned())
    }

    async fn list(
        &self,
        filter: Option<Self::Filter>,
    ) -> Result<Vec<crate::types::Skill>, Error> {
        let skills: Vec<_> = self.skills.read().values().cloned().collect();
        match filter {
            Some(f) => {
                Ok(skills.into_iter().filter(|s| f.matches_skill(s)).collect())
            }
            None => Ok(skills),
        }
    }
}
