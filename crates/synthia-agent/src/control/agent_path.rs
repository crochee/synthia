use std::fmt;

use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentPath(String);

impl AgentPath {
    const SEGMENT_PATTERN: &'static str = r"^[a-z0-9][a-z0-9_-]{0,63}$";

    pub fn new(path: &str) -> Result<Self, String> {
        if !path.starts_with("/root") {
            return Err("AgentPath must start with /root".to_string());
        }
        let re =
            Regex::new(Self::SEGMENT_PATTERN).map_err(|e| e.to_string())?;
        for segment in path.split('/').skip(2) {
            if segment.is_empty() {
                continue;
            }
            if !re.is_match(segment) {
                return Err(format!(
                    "Invalid path segment '{}': must match {}",
                    segment,
                    Self::SEGMENT_PATTERN
                ));
            }
        }
        Ok(Self(path.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_agent_paths() {
        assert!(AgentPath::new("/root").is_ok());
        assert!(AgentPath::new("/root/worker").is_ok());
        assert!(AgentPath::new("/root/worker/sub").is_ok());
        assert!(AgentPath::new("/root/a-b_c").is_ok());
    }

    #[test]
    fn test_invalid_agent_paths() {
        assert!(AgentPath::new("/root/-bad").is_err());
        assert!(AgentPath::new("/root/_bad").is_err());
        assert!(AgentPath::new("root/worker").is_err());
        let too_long = format!("/root/{}", "a".repeat(65));
        assert!(AgentPath::new(&too_long).is_err());
    }

    #[test]
    fn test_display_and_as_str() {
        let path = AgentPath::new("/root/worker").unwrap();
        assert_eq!(path.as_str(), "/root/worker");
        assert_eq!(format!("{}", path), "/root/worker");
    }
}
