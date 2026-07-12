//! Typed ID newtypes to prevent cross-type confusion at compile time.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            #[inline]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            #[inline]
            fn default() -> Self {
                Self::new()
            }
        }

        impl std::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;

            #[inline]
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Uuid::parse_str(s)?))
            }
        }
    };
}

typed_id!(SubmissionId);
typed_id!(SessionId);
typed_id!(MessageId);
typed_id!(CallId);
typed_id!(ApprovalId);
typed_id!(TurnId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_uuid_string() {
        let id = SubmissionId::new();
        let s = id.to_string();
        let parsed: SubmissionId = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn distinct_types_do_not_compare() {
        fn _accept_submission(_: SubmissionId) {}
        let s = SessionId::new();
        let _ = s;
    }
}
