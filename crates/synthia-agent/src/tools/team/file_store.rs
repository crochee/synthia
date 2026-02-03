use super::{
    data::{Team, TeamPatch, TeamStatus, Teammate, TeammateStatus},
    message_store::{MessageFileStore, ProtocolFileStore},
};
use crate::{
    Result,
    tools::storage::{FileStore, Index, IndexEntry, StoragePaths},
};

/// Shared storage for all team-related stores.
/// Ensures a single StoragePaths instance is used across all stores.
#[derive(Debug, Clone)]
pub struct TeamStorage {
    pub teammate_store: TeammateFileStore,
    pub team_store: TeamFileStore,
    pub message_store: MessageFileStore,
    pub protocol_store: ProtocolFileStore,
    paths: StoragePaths,
}

impl TeamStorage {
    pub fn new() -> Self {
        let paths = StoragePaths::new();
        Self::new_with_paths(paths)
    }

    pub fn new_with_paths(paths: StoragePaths) -> Self {
        let paths_clone = paths.clone();
        let paths_clone2 = paths.clone();
        let paths_clone3 = paths.clone();
        Self {
            teammate_store: TeammateFileStore::new(paths_clone),
            team_store: TeamFileStore::new(paths_clone2),
            message_store: MessageFileStore::new(paths_clone3),
            protocol_store: ProtocolFileStore::new(paths.clone()),
            paths,
        }
    }

    pub fn paths(&self) -> &StoragePaths {
        &self.paths
    }
}

impl Default for TeamStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct TeammateFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl TeammateFileStore {
    pub fn new(paths: StoragePaths) -> Self {
        let base = FileStore::new(paths.teammates_dir());
        Self { base, paths }
    }

    pub async fn spawn_teammate(
        &self,
        name: &str,
        role: &str,
    ) -> Result<Teammate> {
        self.base.ensure_dir(&self.paths.teammates_dir()).await?;

        let teammate = Teammate::new(name, role);
        let teammate_path = self.paths.teammate_file(name);
        self.base.write_json(&teammate_path, &teammate).await?;

        self.update_index(&teammate).await?;

        Ok(teammate)
    }

    pub async fn get_teammate(&self, name: &str) -> Result<Option<Teammate>> {
        let teammate_path = self.paths.teammate_file(name);

        if !self.base.file_exists(&teammate_path).await {
            return Ok(None);
        }

        let teammate = self.base.read_json(&teammate_path).await?;
        Ok(Some(teammate))
    }

    pub async fn list_teammates(&self) -> Result<Vec<Teammate>> {
        let teammates_dir = self.paths.teammates_dir();

        if !teammates_dir.exists() {
            return Ok(Vec::new());
        }

        let files = self.base.list_files(&teammates_dir, "json").await?;

        let mut teammates = Vec::new();
        for file in files {
            if file.file_name().is_some_and(|name| name == "index.json") {
                continue;
            }

            match self.base.read_json::<Teammate>(&file).await {
                Ok(teammate) => teammates.push(teammate),
                Err(e) => {
                    tracing::warn!(
                        "Failed to load teammate from {:?}: {}",
                        file,
                        e
                    );
                }
            }
        }

        teammates.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(teammates)
    }

    pub async fn update_teammate_status(
        &self,
        name: &str,
        status: TeammateStatus,
    ) -> Result<()> {
        let mut teammate = match self.get_teammate(name).await? {
            Some(t) => t,
            None => {
                return Err(crate::AgentError::session(format!(
                    "Teammate not found: {name}"
                )));
            }
        };

        teammate.status = status;

        let teammate_path = self.paths.teammate_file(name);
        self.base.write_json(&teammate_path, &teammate).await?;

        self.update_index(&teammate).await?;

        Ok(())
    }

    pub async fn update_teammate(
        &self,
        name: &str,
        team_id: Option<&str>,
        current_task: Option<&str>,
        capabilities: Option<&[String]>,
    ) -> Result<Teammate> {
        let mut teammate = match self.get_teammate(name).await? {
            Some(t) => t,
            None => {
                return Err(crate::AgentError::session(format!(
                    "Teammate not found: {name}"
                )));
            }
        };

        if let Some(tid) = team_id {
            teammate.team_id = Some(tid.to_string());
        }
        if let Some(task) = current_task {
            teammate.current_task = Some(task.to_string());
        }
        if let Some(caps) = capabilities {
            teammate.capabilities = caps.to_vec();
        }

        let teammate_path = self.paths.teammate_file(name);
        self.base.write_json(&teammate_path, &teammate).await?;

        self.update_index(&teammate).await?;

        Ok(teammate)
    }

    pub async fn assign_task_to_teammate(
        &self,
        name: &str,
        task_id: &str,
    ) -> Result<Teammate> {
        let mut teammate = match self.get_teammate(name).await? {
            Some(t) => t,
            None => {
                return Err(crate::AgentError::session(format!(
                    "Teammate not found: {name}"
                )));
            }
        };

        teammate.assign_task(task_id);

        let teammate_path = self.paths.teammate_file(name);
        self.base.write_json(&teammate_path, &teammate).await?;

        self.update_index(&teammate).await?;

        Ok(teammate)
    }

    pub async fn clear_teammate_task(&self, name: &str) -> Result<Teammate> {
        let mut teammate = match self.get_teammate(name).await? {
            Some(t) => t,
            None => {
                return Err(crate::AgentError::session(format!(
                    "Teammate not found: {name}"
                )));
            }
        };

        teammate.clear_task();

        let teammate_path = self.paths.teammate_file(name);
        self.base.write_json(&teammate_path, &teammate).await?;

        self.update_index(&teammate).await?;

        Ok(teammate)
    }

    async fn update_index(&self, teammate: &Teammate) -> Result<()> {
        let index_path = self.paths.teammate_index();

        let mut index = if self.base.file_exists(&index_path).await {
            self.base.read_json(&index_path).await.unwrap_or_default()
        } else {
            Index::new()
        };

        let entry = IndexEntry {
            id: teammate.name.clone(),
            subject: Some(teammate.role.clone()),
            status: teammate.status.as_str().to_string(),
            updated_at: chrono::Utc::now().timestamp(),
        };

        index.add_entry(entry);

        self.base.write_json(&index_path, &index).await
    }
}

impl Default for TeammateFileStore {
    fn default() -> Self {
        Self::new(StoragePaths::new())
    }
}

#[derive(Debug, Clone)]
pub struct TeamFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl TeamFileStore {
    pub fn new(paths: StoragePaths) -> Self {
        let base = FileStore::new(paths.teams_dir());
        Self { base, paths }
    }

    pub async fn create_team(
        &self,
        name: &str,
        lead: Option<&str>,
    ) -> Result<Team> {
        self.base.ensure_dir(&self.paths.teams_dir()).await?;

        let mut team = Team::new(name);
        if let Some(l) = lead {
            team = team.with_lead(l);
        }

        let team_path = self.paths.team_file(&team.team_id);
        self.base.write_json(&team_path, &team).await?;

        self.update_index(&team).await?;

        Ok(team)
    }

    pub async fn get_team(&self, team_id: &str) -> Result<Option<Team>> {
        let team_path = self.paths.team_file(team_id);

        if !self.base.file_exists(&team_path).await {
            return Ok(None);
        }

        let team = self.base.read_json(&team_path).await?;
        Ok(Some(team))
    }

    pub async fn list_teams(&self) -> Result<Vec<Team>> {
        let teams_dir = self.paths.teams_dir();

        if !teams_dir.exists() {
            return Ok(Vec::new());
        }

        let files = self.base.list_files(&teams_dir, "json").await?;

        let mut teams = Vec::new();
        for file in files {
            if file.file_name().is_some_and(|name| name == "index.json") {
                continue;
            }

            match self.base.read_json::<Team>(&file).await {
                Ok(team) => teams.push(team),
                Err(e) => {
                    tracing::warn!(
                        "Failed to load team from {:?}: {}",
                        file,
                        e
                    );
                }
            }
        }

        teams.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(teams)
    }

    pub async fn update_team_status(
        &self,
        team_id: &str,
        status: TeamStatus,
    ) -> Result<()> {
        let mut team = match self.get_team(team_id).await? {
            Some(t) => t,
            None => {
                return Err(crate::AgentError::session(format!(
                    "Team not found: {team_id}"
                )));
            }
        };

        team.status = status;
        team.updated_at = chrono::Utc::now().timestamp() as u64;

        let team_path = self.paths.team_file(team_id);
        self.base.write_json(&team_path, &team).await?;

        self.update_index(&team).await?;

        Ok(())
    }

    pub async fn update_team(
        &self,
        team_id: &str,
        patch: &TeamPatch,
    ) -> Result<Team> {
        let mut team = match self.get_team(team_id).await? {
            Some(t) => t,
            None => {
                return Err(crate::AgentError::session(format!(
                    "Team not found: {team_id}"
                )));
            }
        };

        if let Some(ref name) = patch.name {
            team.name = name.clone();
        }
        if let Some(status) = patch.status {
            team.status = status;
        }
        if let Some(ref lead) = patch.lead {
            team.lead = Some(lead.clone());
        }
        if let Some(ref task_ids) = patch.task_ids {
            team.task_ids = task_ids.clone();
        }

        team.updated_at = chrono::Utc::now().timestamp() as u64;

        let team_path = self.paths.team_file(team_id);
        self.base.write_json(&team_path, &team).await?;

        self.update_index(&team).await?;

        Ok(team)
    }

    pub async fn assign_task_to_team(
        &self,
        team_id: &str,
        task_id: &str,
    ) -> Result<Team> {
        let mut team = match self.get_team(team_id).await? {
            Some(t) => t,
            None => {
                return Err(crate::AgentError::session(format!(
                    "Team not found: {team_id}"
                )));
            }
        };

        team.add_task(task_id);

        let team_path = self.paths.team_file(team_id);
        self.base.write_json(&team_path, &team).await?;

        self.update_index(&team).await?;

        Ok(team)
    }

    pub async fn remove_task_from_team(
        &self,
        team_id: &str,
        task_id: &str,
    ) -> Result<Team> {
        let mut team = match self.get_team(team_id).await? {
            Some(t) => t,
            None => {
                return Err(crate::AgentError::session(format!(
                    "Team not found: {team_id}"
                )));
            }
        };

        team.remove_task(task_id);

        let team_path = self.paths.team_file(team_id);
        self.base.write_json(&team_path, &team).await?;

        self.update_index(&team).await?;

        Ok(team)
    }

    pub async fn delete_team(&self, team_id: &str) -> Result<()> {
        let team_path = self.paths.team_file(team_id);
        self.base.delete_file(&team_path).await?;

        self.remove_from_index(team_id).await?;

        Ok(())
    }

    async fn update_index(&self, team: &Team) -> Result<()> {
        let index_path = self.paths.team_index();

        let mut index = if self.base.file_exists(&index_path).await {
            self.base.read_json(&index_path).await.unwrap_or_default()
        } else {
            Index::new()
        };

        let entry = IndexEntry {
            id: team.team_id.clone(),
            subject: Some(team.name.clone()),
            status: team.status.as_str().to_string(),
            updated_at: team.updated_at as i64,
        };

        index.add_entry(entry);

        self.base.write_json(&index_path, &index).await
    }

    async fn remove_from_index(&self, team_id: &str) -> Result<()> {
        let index_path = self.paths.team_index();

        if !self.base.file_exists(&index_path).await {
            return Ok(());
        }

        let mut index: Index = self.base.read_json(&index_path).await?;
        index.remove_entry(team_id);

        self.base.write_json(&index_path, &index).await
    }
}

impl Default for TeamFileStore {
    fn default() -> Self {
        Self::new(StoragePaths::new())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn create_test_storage(base: &std::path::Path) -> StoragePaths {
        StoragePaths::with_base(base.to_path_buf())
    }

    #[tokio::test]
    async fn test_team_storage_new() {
        let _dir = tempdir().unwrap();
        let storage = TeamStorage::new();
        assert!(storage.paths().teammates_dir().ends_with("teammates"));
    }

    #[tokio::test]
    async fn test_teammate_file_store_spawn_teammate() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeammateFileStore::new(paths);

        let teammate =
            store.spawn_teammate("alice", "developer").await.unwrap();
        assert_eq!(teammate.name, "alice");
        assert_eq!(teammate.role, "developer");
        assert_eq!(teammate.status, TeammateStatus::Working);
    }

    #[tokio::test]
    async fn test_teammate_file_store_get_teammate() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeammateFileStore::new(paths);

        store.spawn_teammate("bob", "tester").await.unwrap();

        let teammate = store.get_teammate("bob").await.unwrap();
        assert!(teammate.is_some());
        let t = teammate.unwrap();
        assert_eq!(t.name, "bob");
        assert_eq!(t.role, "tester");
    }

    #[tokio::test]
    async fn test_teammate_file_store_get_nonexistent() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeammateFileStore::new(paths);

        let teammate = store.get_teammate("nonexistent").await.unwrap();
        assert!(teammate.is_none());
    }

    #[tokio::test]
    async fn test_teammate_file_store_list_teammates() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeammateFileStore::new(paths);

        store.spawn_teammate("alice", "dev").await.unwrap();
        store.spawn_teammate("bob", "test").await.unwrap();

        let teammates = store.list_teammates().await.unwrap();
        assert_eq!(teammates.len(), 2);
    }

    #[tokio::test]
    async fn test_teammate_file_store_update_status() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeammateFileStore::new(paths);

        store.spawn_teammate("carol", "dev").await.unwrap();

        store
            .update_teammate_status("carol", TeammateStatus::Idle)
            .await
            .unwrap();

        let teammate = store.get_teammate("carol").await.unwrap().unwrap();
        assert_eq!(teammate.status, TeammateStatus::Idle);
    }

    #[tokio::test]
    async fn test_teammate_file_store_assign_task() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeammateFileStore::new(paths);

        store.spawn_teammate("dave", "dev").await.unwrap();

        let teammate = store
            .assign_task_to_teammate("dave", "task-1")
            .await
            .unwrap();
        assert_eq!(teammate.current_task, Some("task-1".to_string()));
        assert_eq!(teammate.status, TeammateStatus::Working);
    }

    #[tokio::test]
    async fn test_teammate_file_store_clear_task() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeammateFileStore::new(paths);

        store.spawn_teammate("eve", "dev").await.unwrap();
        store
            .assign_task_to_teammate("eve", "task-1")
            .await
            .unwrap();

        let teammate = store.clear_teammate_task("eve").await.unwrap();
        assert!(teammate.current_task.is_none());
        assert_eq!(teammate.status, TeammateStatus::Idle);
    }

    #[tokio::test]
    async fn test_team_file_store_create_team() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeamFileStore::new(paths);

        let team = store.create_team("Alpha", Some("lead-1")).await.unwrap();
        assert_eq!(team.name, "Alpha");
        assert_eq!(team.lead, Some("lead-1".to_string()));
        assert_eq!(team.status, TeamStatus::Created);
    }

    #[tokio::test]
    async fn test_team_file_store_get_team() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeamFileStore::new(paths);

        let created = store.create_team("Beta", None).await.unwrap();

        let team = store.get_team(&created.team_id).await.unwrap();
        assert!(team.is_some());
        assert_eq!(team.unwrap().name, "Beta");
    }

    #[tokio::test]
    async fn test_team_file_store_get_nonexistent() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeamFileStore::new(paths);

        let team = store.get_team("nonexistent-id").await.unwrap();
        assert!(team.is_none());
    }

    #[tokio::test]
    async fn test_team_file_store_list_teams() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeamFileStore::new(paths);

        store.create_team("Team-A", None).await.unwrap();
        store.create_team("Team-B", None).await.unwrap();

        let teams = store.list_teams().await.unwrap();
        assert_eq!(teams.len(), 2);
    }

    #[tokio::test]
    async fn test_team_file_store_update_status() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeamFileStore::new(paths);

        let team = store.create_team("Gamma", None).await.unwrap();

        store
            .update_team_status(&team.team_id, TeamStatus::Running)
            .await
            .unwrap();

        let updated = store.get_team(&team.team_id).await.unwrap().unwrap();
        assert_eq!(updated.status, TeamStatus::Running);
    }

    #[tokio::test]
    async fn test_team_file_store_assign_task() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeamFileStore::new(paths);

        let team = store.create_team("Delta", None).await.unwrap();

        let updated = store
            .assign_task_to_team(&team.team_id, "task-1")
            .await
            .unwrap();
        assert!(updated.task_ids.contains(&"task-1".to_string()));
    }

    #[tokio::test]
    async fn test_team_file_store_remove_task() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeamFileStore::new(paths);

        let team = store.create_team("Epsilon", None).await.unwrap();
        store
            .assign_task_to_team(&team.team_id, "task-1")
            .await
            .unwrap();

        let updated = store
            .remove_task_from_team(&team.team_id, "task-1")
            .await
            .unwrap();
        assert!(!updated.task_ids.contains(&"task-1".to_string()));
    }

    #[tokio::test]
    async fn test_team_file_store_delete_team() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeamFileStore::new(paths);

        let team = store.create_team("Zeta", None).await.unwrap();
        assert!(store.get_team(&team.team_id).await.unwrap().is_some());

        store.delete_team(&team.team_id).await.unwrap();
        assert!(store.get_team(&team.team_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_team_file_store_update_team_with_patch() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = TeamFileStore::new(paths);

        let team = store.create_team("Eta", None).await.unwrap();

        let patch = TeamPatch {
            name: Some("Eta-Renamed".to_string()),
            status: Some(TeamStatus::Running),
            lead: Some("new-lead".to_string()),
            task_ids: None,
        };

        let updated = store.update_team(&team.team_id, &patch).await.unwrap();
        assert_eq!(updated.name, "Eta-Renamed");
        assert_eq!(updated.status, TeamStatus::Running);
        assert_eq!(updated.lead, Some("new-lead".to_string()));
    }
}
