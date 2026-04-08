use super::{
    data::{MessagePriority, MessageType, TeamMessage},
    file_store::TeammateFileStore,
};
use crate::{
    Result,
    tools::storage::{FileStore, StoragePaths},
};

#[derive(Debug, Clone)]
pub struct MessageFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl MessageFileStore {
    pub fn new(paths: StoragePaths) -> Self {
        let base = FileStore::new(paths.messages_dir());
        Self { base, paths }
    }

    pub async fn send_message(
        &self,
        recipient: &str,
        msg_type: MessageType,
        sender: &str,
        content: &str,
        request_id: Option<&str>,
    ) -> Result<()> {
        self.base.ensure_dir(&self.paths.messages_dir()).await?;

        let mut message =
            TeamMessage::new(recipient, msg_type, sender, content);
        if let Some(rid) = request_id {
            message = message.with_request_id(rid);
        }

        let message_path = self.paths.message_file(recipient);
        self.base.append_jsonl(&message_path, &message).await?;

        Ok(())
    }

    pub async fn read_inbox(
        &self,
        recipient: &str,
    ) -> Result<Vec<TeamMessage>> {
        let message_path = self.paths.message_file(recipient);

        if !message_path.exists() {
            return Ok(Vec::new());
        }

        let messages = self.base.read_jsonl(&message_path).await?;
        Ok(messages)
    }

    pub async fn mark_messages_read(&self, recipient: &str) -> Result<()> {
        let message_path = self.paths.message_file(recipient);

        if !message_path.exists() {
            return Ok(());
        }

        let mut messages: Vec<TeamMessage> =
            self.base.read_jsonl(&message_path).await?;

        for msg in &mut messages {
            msg.read = true;
        }

        let mut content = String::new();
        for msg in &messages {
            let line = serde_json::to_string(msg).map_err(|e| {
                crate::AgentError::internal(format!(
                    "Failed to serialize message: {e}"
                ))
            })?;
            content.push_str(&line);
            content.push('\n');
        }

        self.base.atomic_write(&message_path, &content).await?;

        Ok(())
    }

    /// Broadcast a message to all teammates except the sender.
    /// Takes a reference to TeammateFileStore to avoid creating a new instance.
    pub async fn broadcast(
        &self,
        sender: &str,
        content: &str,
        teammate_store: &TeammateFileStore,
    ) -> Result<usize> {
        let teammates = teammate_store.list_teammates().await?;

        let mut count = 0;
        for teammate in teammates {
            if teammate.name != sender {
                self.send_message(
                    &teammate.name,
                    MessageType::Broadcast,
                    sender,
                    content,
                    None,
                )
                .await?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Send a message with priority to a recipient's inbox.
    pub async fn send_message_with_priority(
        &self,
        recipient: &str,
        msg_type: MessageType,
        sender: &str,
        content: &str,
        priority: MessagePriority,
        request_id: Option<&str>,
    ) -> Result<()> {
        self.base.ensure_dir(&self.paths.messages_dir()).await?;

        let mut message =
            TeamMessage::new(recipient, msg_type, sender, content)
                .with_priority(priority);
        if let Some(rid) = request_id {
            message = message.with_request_id(rid);
        }

        let message_path = self.paths.message_file(recipient);
        self.base.append_jsonl(&message_path, &message).await?;

        Ok(())
    }

    /// Read inbox with message type filtering.
    /// Returns messages filtered by the specified message types.
    pub async fn read_inbox_filtered(
        &self,
        recipient: &str,
        msg_types: &[MessageType],
    ) -> Result<Vec<TeamMessage>> {
        let messages = self.read_inbox(recipient).await?;

        let filtered: Vec<TeamMessage> = messages
            .into_iter()
            .filter(|msg| msg_types.contains(&msg.msg_type))
            .collect();

        Ok(filtered)
    }

    /// Read inbox sorted by priority (high to low).
    /// Messages with the same priority are sorted by timestamp (newest first).
    pub async fn read_inbox_by_priority(
        &self,
        recipient: &str,
    ) -> Result<Vec<TeamMessage>> {
        let mut messages = self.read_inbox(recipient).await?;

        // Sort by priority (ascending, High first since High=0 < Normal=1 < Low=2)
        // then by timestamp (descending, newest first)
        messages.sort_by(|a, b| match a.priority.cmp(&b.priority) {
            std::cmp::Ordering::Equal => b
                .timestamp
                .partial_cmp(&a.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal),
            other => other,
        });

        Ok(messages)
    }

    /// Read Lead's inbox - receives messages from Members.
    /// This is a convenience method for reading the lead's inbox with priority sorting.
    pub async fn read_lead_inbox(
        &self,
        lead_name: &str,
    ) -> Result<Vec<TeamMessage>> {
        self.read_inbox_by_priority(lead_name).await
    }

    /// Read unread messages for a recipient.
    pub async fn read_unread_messages(
        &self,
        recipient: &str,
    ) -> Result<Vec<TeamMessage>> {
        let messages = self.read_inbox(recipient).await?;

        let unread: Vec<TeamMessage> =
            messages.into_iter().filter(|msg| !msg.read).collect();

        Ok(unread)
    }

    /// Read unread messages filtered by type.
    pub async fn read_unread_messages_filtered(
        &self,
        recipient: &str,
        msg_types: &[MessageType],
    ) -> Result<Vec<TeamMessage>> {
        let messages = self.read_unread_messages(recipient).await?;

        let filtered: Vec<TeamMessage> = messages
            .into_iter()
            .filter(|msg| msg_types.contains(&msg.msg_type))
            .collect();

        Ok(filtered)
    }

    /// Count unread messages for a recipient.
    pub async fn count_unread(&self, recipient: &str) -> Result<usize> {
        let messages = self.read_unread_messages(recipient).await?;
        Ok(messages.len())
    }

    /// Count messages by type for a recipient.
    pub async fn count_by_type(
        &self,
        recipient: &str,
        msg_type: MessageType,
    ) -> Result<usize> {
        let messages = self.read_inbox(recipient).await?;
        let count = messages
            .iter()
            .filter(|msg| msg.msg_type == msg_type)
            .count();
        Ok(count)
    }

    /// Get latest message of a specific type for a recipient.
    pub async fn get_latest_by_type(
        &self,
        recipient: &str,
        msg_type: MessageType,
    ) -> Result<Option<TeamMessage>> {
        let messages = self.read_inbox_filtered(recipient, &[msg_type]).await?;

        let latest = messages.into_iter().max_by(|a, b| {
            a.timestamp
                .partial_cmp(&b.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(latest)
    }

    /// Clear all messages for a recipient.
    pub async fn clear_inbox(&self, recipient: &str) -> Result<()> {
        let message_path = self.paths.message_file(recipient);

        if message_path.exists() {
            std::fs::remove_file(&message_path).map_err(|e| {
                crate::AgentError::internal(format!(
                    "Failed to clear inbox: {e}"
                ))
            })?;
        }

        Ok(())
    }
}

impl Default for MessageFileStore {
    fn default() -> Self {
        Self::new(StoragePaths::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownRequestData {
    pub request_id: String,
    pub target: String,
    pub status: String,
    pub created_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRequestData {
    pub request_id: String,
    pub sender: String,
    pub plan: String,
    pub status: String,
    pub created_at: f64,
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct ProtocolFileStore {
    base: FileStore,
    paths: StoragePaths,
}

impl ProtocolFileStore {
    pub fn new(paths: StoragePaths) -> Self {
        let base = FileStore::new(paths.protocol_dir());
        Self { base, paths }
    }

    pub async fn create_shutdown_request(
        &self,
        request_id: &str,
        target: &str,
    ) -> Result<()> {
        self.base.ensure_dir(&self.paths.protocol_dir()).await?;

        let request = ShutdownRequestData {
            request_id: request_id.to_string(),
            target: target.to_string(),
            status: "pending".to_string(),
            created_at: chrono::Utc::now().timestamp() as f64,
        };

        let mut requests = self.list_shutdown_requests().await?;
        requests.push(request);

        let requests_path = self.paths.shutdown_requests_file();
        self.base.write_json(&requests_path, &requests).await?;

        Ok(())
    }

    pub async fn get_shutdown_request(
        &self,
        request_id: &str,
    ) -> Result<Option<ShutdownRequestData>> {
        let requests = self.list_shutdown_requests().await?;

        Ok(requests.into_iter().find(|r| r.request_id == request_id))
    }

    pub async fn update_shutdown_status(
        &self,
        request_id: &str,
        status: &str,
    ) -> Result<()> {
        let mut requests = self.list_shutdown_requests().await?;

        for request in &mut requests {
            if request.request_id == request_id {
                request.status = status.to_string();
            }
        }

        let requests_path = self.paths.shutdown_requests_file();
        self.base.write_json(&requests_path, &requests).await?;

        Ok(())
    }

    async fn list_shutdown_requests(&self) -> Result<Vec<ShutdownRequestData>> {
        let requests_path = self.paths.shutdown_requests_file();

        if !self.base.file_exists(&requests_path).await {
            return Ok(Vec::new());
        }

        self.base.read_json(&requests_path).await
    }

    pub async fn create_plan_request(
        &self,
        request_id: &str,
        sender: &str,
        plan: &str,
    ) -> Result<()> {
        self.base.ensure_dir(&self.paths.protocol_dir()).await?;

        let request = PlanRequestData {
            request_id: request_id.to_string(),
            sender: sender.to_string(),
            plan: plan.to_string(),
            status: "pending".to_string(),
            created_at: chrono::Utc::now().timestamp() as f64,
        };

        let mut requests = self.list_plan_requests().await?;
        requests.push(request);

        let requests_path = self.paths.plan_requests_file();
        self.base.write_json(&requests_path, &requests).await?;

        Ok(())
    }

    pub async fn get_plan_request(
        &self,
        request_id: &str,
    ) -> Result<Option<PlanRequestData>> {
        let requests = self.list_plan_requests().await?;

        Ok(requests.into_iter().find(|r| r.request_id == request_id))
    }

    pub async fn update_plan_status(
        &self,
        request_id: &str,
        status: &str,
    ) -> Result<()> {
        let mut requests = self.list_plan_requests().await?;

        for request in &mut requests {
            if request.request_id == request_id {
                request.status = status.to_string();
            }
        }

        let requests_path = self.paths.plan_requests_file();
        self.base.write_json(&requests_path, &requests).await?;

        Ok(())
    }

    async fn list_plan_requests(&self) -> Result<Vec<PlanRequestData>> {
        let requests_path = self.paths.plan_requests_file();

        if !self.base.file_exists(&requests_path).await {
            return Ok(Vec::new());
        }

        self.base.read_json(&requests_path).await
    }
}

impl Default for ProtocolFileStore {
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
    async fn test_message_file_store_send_message() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message("alice", MessageType::Message, "bob", "Hello", None)
            .await
            .unwrap();

        let messages = store.read_inbox("alice").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[0].sender, "bob");
    }

    #[tokio::test]
    async fn test_message_file_store_send_message_with_request_id() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message(
                "alice",
                MessageType::Broadcast,
                "bob",
                "Hi all",
                Some("req-1"),
            )
            .await
            .unwrap();

        let messages = store.read_inbox("alice").await.unwrap();
        assert_eq!(messages[0].request_id, Some("req-1".to_string()));
    }

    #[tokio::test]
    async fn test_message_file_store_read_inbox_empty() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        let messages = store.read_inbox("nonexistent").await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_message_file_store_mark_messages_read() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message("alice", MessageType::Message, "bob", "Hello", None)
            .await
            .unwrap();

        let before = store.read_inbox("alice").await.unwrap();
        assert!(!before[0].read);

        store.mark_messages_read("alice").await.unwrap();

        let after = store.read_inbox("alice").await.unwrap();
        assert!(after[0].read);
    }

    #[tokio::test]
    async fn test_message_file_store_mark_messages_read_nonexistent() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store.mark_messages_read("nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn test_message_file_store_broadcast() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let message_store = MessageFileStore::new(paths.clone());
        let teammate_store = TeammateFileStore::new(paths);

        teammate_store.spawn_teammate("alice", "dev").await.unwrap();
        teammate_store.spawn_teammate("bob", "dev").await.unwrap();

        let count = message_store
            .broadcast("lead", "Broadcast message", &teammate_store)
            .await
            .unwrap();
        assert_eq!(count, 2);

        let alice_msgs = message_store.read_inbox("alice").await.unwrap();
        assert_eq!(alice_msgs.len(), 1);
        assert_eq!(alice_msgs[0].content, "Broadcast message");
        assert_eq!(alice_msgs[0].sender, "lead");
    }

    #[tokio::test]
    async fn test_protocol_file_store_create_shutdown_request() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = ProtocolFileStore::new(paths);

        store
            .create_shutdown_request("req-1", "alice")
            .await
            .unwrap();

        let request = store.get_shutdown_request("req-1").await.unwrap();
        assert!(request.is_some());
        let r = request.unwrap();
        assert_eq!(r.request_id, "req-1");
        assert_eq!(r.target, "alice");
        assert_eq!(r.status, "pending");
    }

    #[tokio::test]
    async fn test_protocol_file_store_get_nonexistent_shutdown_request() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = ProtocolFileStore::new(paths);

        let request = store.get_shutdown_request("nonexistent").await.unwrap();
        assert!(request.is_none());
    }

    #[tokio::test]
    async fn test_protocol_file_store_update_shutdown_status() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = ProtocolFileStore::new(paths);

        store.create_shutdown_request("req-2", "bob").await.unwrap();
        store
            .update_shutdown_status("req-2", "approved")
            .await
            .unwrap();

        let request =
            store.get_shutdown_request("req-2").await.unwrap().unwrap();
        assert_eq!(request.status, "approved");
    }

    #[tokio::test]
    async fn test_protocol_file_store_create_plan_request() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = ProtocolFileStore::new(paths);

        store
            .create_plan_request("plan-1", "alice", "Do task X")
            .await
            .unwrap();

        let request = store.get_plan_request("plan-1").await.unwrap();
        assert!(request.is_some());
        let r = request.unwrap();
        assert_eq!(r.request_id, "plan-1");
        assert_eq!(r.sender, "alice");
        assert_eq!(r.plan, "Do task X");
        assert_eq!(r.status, "pending");
    }

    #[tokio::test]
    async fn test_protocol_file_store_update_plan_status() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = ProtocolFileStore::new(paths);

        store
            .create_plan_request("plan-2", "bob", "Do task Y")
            .await
            .unwrap();
        store
            .update_plan_status("plan-2", "rejected")
            .await
            .unwrap();

        let request = store.get_plan_request("plan-2").await.unwrap().unwrap();
        assert_eq!(request.status, "rejected");
    }

    #[tokio::test]
    async fn test_protocol_file_store_list_plan_requests_empty() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = ProtocolFileStore::new(paths);

        let requests = store.list_plan_requests().await.unwrap();
        assert!(requests.is_empty());
    }

    // ===== Message Queue Integration Tests =====

    #[tokio::test]
    async fn test_message_file_store_send_message_with_priority() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message_with_priority(
                "lead",
                MessageType::TaskFailed,
                "member1",
                "Task X failed",
                MessagePriority::High,
                None,
            )
            .await
            .unwrap();

        let messages = store.read_inbox("lead").await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].priority, MessagePriority::High);
        assert_eq!(messages[0].msg_type, MessageType::TaskFailed);
    }

    #[tokio::test]
    async fn test_message_file_store_read_inbox_filtered() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        // Send different types of messages
        store
            .send_message(
                "lead",
                MessageType::TaskAssigned,
                "system",
                "Task 1",
                None,
            )
            .await
            .unwrap();
        store
            .send_message(
                "lead",
                MessageType::TaskCompleted,
                "member1",
                "Done",
                None,
            )
            .await
            .unwrap();
        store
            .send_message(
                "lead",
                MessageType::TaskFailed,
                "member2",
                "Failed",
                None,
            )
            .await
            .unwrap();
        store
            .send_message(
                "lead",
                MessageType::StatusUpdate,
                "member1",
                "Working",
                None,
            )
            .await
            .unwrap();

        // Filter for task-related messages
        let task_messages = store
            .read_inbox_filtered(
                "lead",
                &[
                    MessageType::TaskAssigned,
                    MessageType::TaskCompleted,
                    MessageType::TaskFailed,
                ],
            )
            .await
            .unwrap();

        assert_eq!(task_messages.len(), 3);

        // Filter for completion messages only
        let completed = store
            .read_inbox_filtered("lead", &[MessageType::TaskCompleted])
            .await
            .unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].content, "Done");
    }

    #[tokio::test]
    async fn test_message_file_store_read_inbox_by_priority() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        // Send messages with different priorities
        store
            .send_message_with_priority(
                "lead",
                MessageType::StatusUpdate,
                "member1",
                "Status low",
                MessagePriority::Low,
                None,
            )
            .await
            .unwrap();

        // Small delay to ensure different timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        store
            .send_message_with_priority(
                "lead",
                MessageType::TaskAssigned,
                "system",
                "Task normal",
                MessagePriority::Normal,
                None,
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        store
            .send_message_with_priority(
                "lead",
                MessageType::TaskFailed,
                "member2",
                "Task failed",
                MessagePriority::High,
                None,
            )
            .await
            .unwrap();

        let messages = store.read_inbox_by_priority("lead").await.unwrap();
        assert_eq!(messages.len(), 3);

        // High priority should be first
        assert_eq!(messages[0].priority, MessagePriority::High);
        assert_eq!(messages[0].msg_type, MessageType::TaskFailed);

        // Normal priority should be second
        assert_eq!(messages[1].priority, MessagePriority::Normal);

        // Low priority should be last
        assert_eq!(messages[2].priority, MessagePriority::Low);
    }

    #[tokio::test]
    async fn test_message_file_store_read_lead_inbox() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        // Member sends task completion to Lead
        store
            .send_message_with_priority(
                "lead",
                MessageType::TaskCompleted,
                "member1",
                "Task completed successfully",
                MessagePriority::Normal,
                None,
            )
            .await
            .unwrap();

        // Member sends failure to Lead
        store
            .send_message_with_priority(
                "lead",
                MessageType::TaskFailed,
                "member2",
                "Task failed critically",
                MessagePriority::High,
                None,
            )
            .await
            .unwrap();

        let lead_inbox = store.read_lead_inbox("lead").await.unwrap();
        assert_eq!(lead_inbox.len(), 2);

        // High priority failure should be first
        assert_eq!(lead_inbox[0].msg_type, MessageType::TaskFailed);
        assert_eq!(lead_inbox[0].priority, MessagePriority::High);
    }

    #[tokio::test]
    async fn test_message_file_store_read_unread_messages() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message("alice", MessageType::Message, "bob", "Hello", None)
            .await
            .unwrap();
        store
            .send_message("alice", MessageType::Message, "carol", "Hi", None)
            .await
            .unwrap();

        let unread = store.read_unread_messages("alice").await.unwrap();
        assert_eq!(unread.len(), 2);

        // Mark as read
        store.mark_messages_read("alice").await.unwrap();

        let unread_after = store.read_unread_messages("alice").await.unwrap();
        assert!(unread_after.is_empty());
    }

    #[tokio::test]
    async fn test_message_file_store_read_unread_messages_filtered() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message(
                "lead",
                MessageType::TaskCompleted,
                "member1",
                "Done",
                None,
            )
            .await
            .unwrap();
        store
            .send_message(
                "lead",
                MessageType::StatusUpdate,
                "member2",
                "Working",
                None,
            )
            .await
            .unwrap();

        let unread_tasks = store
            .read_unread_messages_filtered(
                "lead",
                &[MessageType::TaskCompleted, MessageType::TaskFailed],
            )
            .await
            .unwrap();

        assert_eq!(unread_tasks.len(), 1);
        assert_eq!(unread_tasks[0].msg_type, MessageType::TaskCompleted);
    }

    #[tokio::test]
    async fn test_message_file_store_count_unread() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message("alice", MessageType::Message, "bob", "Hello", None)
            .await
            .unwrap();
        store
            .send_message("alice", MessageType::Message, "carol", "Hi", None)
            .await
            .unwrap();

        let count = store.count_unread("alice").await.unwrap();
        assert_eq!(count, 2);

        store.mark_messages_read("alice").await.unwrap();

        let count_after = store.count_unread("alice").await.unwrap();
        assert_eq!(count_after, 0);
    }

    #[tokio::test]
    async fn test_message_file_store_count_by_type() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message(
                "lead",
                MessageType::TaskCompleted,
                "member1",
                "Done 1",
                None,
            )
            .await
            .unwrap();
        store
            .send_message(
                "lead",
                MessageType::TaskCompleted,
                "member2",
                "Done 2",
                None,
            )
            .await
            .unwrap();
        store
            .send_message(
                "lead",
                MessageType::TaskFailed,
                "member3",
                "Failed",
                None,
            )
            .await
            .unwrap();

        let completed_count = store
            .count_by_type("lead", MessageType::TaskCompleted)
            .await
            .unwrap();
        assert_eq!(completed_count, 2);

        let failed_count = store
            .count_by_type("lead", MessageType::TaskFailed)
            .await
            .unwrap();
        assert_eq!(failed_count, 1);

        let assigned_count = store
            .count_by_type("lead", MessageType::TaskAssigned)
            .await
            .unwrap();
        assert_eq!(assigned_count, 0);
    }

    #[tokio::test]
    async fn test_message_file_store_get_latest_by_type() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message(
                "lead",
                MessageType::TaskCompleted,
                "member1",
                "First completion",
                None,
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        store
            .send_message(
                "lead",
                MessageType::TaskCompleted,
                "member2",
                "Second completion",
                None,
            )
            .await
            .unwrap();

        let latest = store
            .get_latest_by_type("lead", MessageType::TaskCompleted)
            .await
            .unwrap();

        assert!(latest.is_some());
        let latest = latest.unwrap();
        assert_eq!(latest.content, "Second completion");
        assert_eq!(latest.sender, "member2");
    }

    #[tokio::test]
    async fn test_message_file_store_clear_inbox() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        store
            .send_message("alice", MessageType::Message, "bob", "Hello", None)
            .await
            .unwrap();

        let messages = store.read_inbox("alice").await.unwrap();
        assert_eq!(messages.len(), 1);

        store.clear_inbox("alice").await.unwrap();

        let messages_after = store.read_inbox("alice").await.unwrap();
        assert!(messages_after.is_empty());
    }

    #[tokio::test]
    async fn test_message_priority_ordering() {
        let dir = tempdir().unwrap();
        let paths = create_test_storage(dir.path());
        let store = MessageFileStore::new(paths);

        // Send multiple messages with different priorities
        // Use delays to ensure different timestamps
        store
            .send_message_with_priority(
                "lead",
                MessageType::StatusUpdate,
                "member1",
                "Low priority update",
                MessagePriority::Low,
                None,
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        store
            .send_message_with_priority(
                "lead",
                MessageType::TaskAssigned,
                "system",
                "Normal priority task",
                MessagePriority::Normal,
                None,
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        store
            .send_message_with_priority(
                "lead",
                MessageType::TaskFailed,
                "member2",
                "High priority failure",
                MessagePriority::High,
                None,
            )
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        store
            .send_message_with_priority(
                "lead",
                MessageType::Message,
                "member3",
                "Another normal message",
                MessagePriority::Normal,
                None,
            )
            .await
            .unwrap();

        let messages = store.read_inbox_by_priority("lead").await.unwrap();
        assert_eq!(messages.len(), 4);

        // Verify priority ordering
        assert_eq!(messages[0].priority, MessagePriority::High);
        assert_eq!(messages[0].content, "High priority failure");

        // Normal priority messages should be sorted by timestamp (newest first)
        assert_eq!(messages[1].priority, MessagePriority::Normal);
        assert_eq!(messages[1].content, "Another normal message");
        assert_eq!(messages[2].priority, MessagePriority::Normal);
        assert_eq!(messages[2].content, "Normal priority task");

        assert_eq!(messages[3].priority, MessagePriority::Low);
        assert_eq!(messages[3].content, "Low priority update");
    }
}
