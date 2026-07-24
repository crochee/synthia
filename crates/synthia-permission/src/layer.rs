use serde::{Deserialize, Serialize};

/// Permission rule layer with priority: User > Agent > Default.
/// Lower numeric value = lower priority (evaluated later in merge).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Hash,
    Default,
)]
#[repr(u8)]
pub enum RuleLayer {
    #[default]
    Default = 0,
    Agent = 1,
    User = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_priority_order() {
        assert!(RuleLayer::User > RuleLayer::Agent);
        assert!(RuleLayer::Agent > RuleLayer::Default);
    }

    #[test]
    fn test_layer_default() {
        assert_eq!(RuleLayer::default(), RuleLayer::Default);
    }
}
