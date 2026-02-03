//! System notification types

use serde::{Deserialize, Serialize};

/// System notification type
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum SystemNotificationType {
    #[default]
    InlineMessage,
    Progress,
    Log,
}

/// System notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemNotification {
    pub notification_type: SystemNotificationType,
    pub msg: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_type_default() {
        let nt = SystemNotificationType::default();
        assert!(matches!(nt, SystemNotificationType::InlineMessage));
    }

    #[test]
    fn test_notification_debug() {
        let nt = SystemNotificationType::Progress;
        let debug = format!("{nt:?}");
        assert!(debug.contains("Progress"));
    }

    #[test]
    fn test_system_notification_new() {
        let notification = SystemNotification {
            notification_type: SystemNotificationType::InlineMessage,
            msg: "Test message".to_string(),
            data: None,
        };
        assert_eq!(notification.msg, "Test message");
        assert!(notification.data.is_none());
    }

    #[test]
    fn test_system_notification_with_data() {
        let notification = SystemNotification {
            notification_type: SystemNotificationType::Progress,
            msg: "Working...".to_string(),
            data: Some(serde_json::json!({"progress": 50})),
        };
        assert!(notification.data.is_some());
        assert_eq!(notification.data.unwrap()["progress"], 50);
    }

    #[test]
    fn test_system_notification_clone() {
        let original = SystemNotification {
            notification_type: SystemNotificationType::Log,
            msg: "Log message".to_string(),
            data: None,
        };
        let cloned = original.clone();
        assert_eq!(cloned.msg, original.msg);
        assert_eq!(cloned.notification_type, original.notification_type);
    }

    #[test]
    fn test_notification_serialization() {
        let notification = SystemNotification {
            notification_type: SystemNotificationType::Progress,
            msg: "Test".to_string(),
            data: None,
        };
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("Progress"));
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_notification_deserialization() {
        let json = r#"{"notification_type":"Log","msg":"Hello"}"#;
        let notification: SystemNotification =
            serde_json::from_str(json).unwrap();
        assert!(matches!(
            notification.notification_type,
            SystemNotificationType::Log
        ));
        assert_eq!(notification.msg, "Hello");
    }
}
