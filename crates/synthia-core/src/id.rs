use chrono::{DateTime, Utc};
use ulid::Ulid;

pub fn generate_session_id() -> String {
    Ulid::new().to_string()
}

pub fn generate_tool_call_id() -> String {
    Ulid::new().to_string()
}

pub fn generate_task_id() -> String {
    Ulid::new().to_string()
}

pub fn generate_message_id() -> String {
    Ulid::new().to_string()
}

pub fn extract_timestamp(id: &str) -> Option<DateTime<Utc>> {
    Ulid::from_string(id).ok().map(|ulid| {
        let dt: chrono::DateTime<Utc> = ulid.datetime().into();
        dt
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn test_generate_session_id_is_unique() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_session_id_length() {
        let id = generate_session_id();
        assert_eq!(id.len(), 26);
    }

    #[test]
    fn test_generate_tool_call_id_is_unique() {
        let id1 = generate_tool_call_id();
        let id2 = generate_tool_call_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_task_id_is_unique() {
        let id1 = generate_task_id();
        let id2 = generate_task_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_message_id_is_unique() {
        let id1 = generate_message_id();
        let id2 = generate_message_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_extract_timestamp_from_valid_ulid() {
        let id = generate_session_id();
        let timestamp = extract_timestamp(&id);
        assert!(timestamp.is_some());
        let ts = timestamp.unwrap();
        let now = Utc::now();
        let diff = now - ts;
        assert!(diff.num_seconds() < 5);
    }

    #[test]
    fn test_extract_timestamp_from_invalid() {
        assert!(extract_timestamp("not-a-valid-ulid").is_none());
    }

    #[test]
    fn test_ulid_ids_are_sortable() {
        let id1 = generate_session_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = generate_session_id();
        assert!(id1 < id2);
    }

    #[test]
    fn test_generate_many_ids_are_unique() {
        let mut ids = HashSet::new();
        for _ in 0..1000 {
            let id = generate_session_id();
            assert!(ids.insert(id), "Duplicate ID generated");
        }
    }
}
