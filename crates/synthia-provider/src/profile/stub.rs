//! [`StubProfile`] — the offline provider used for tests and tutorials.

use serde::{Deserialize, Serialize};

/// Typed stub provider profile for offline tests and tutorials.
///
/// Resolves to a
/// [`ModelProviderStub`](crate::traits_stub::ModelProviderStub) that
/// emits canned text without
/// touching the network. Lib consumers use `ProviderProfile::Stub` to
/// assemble a complete agent that runs end-to-end with **zero**
/// API keys.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StubProfile {
    pub name: String,
    /// Default model id advertised on the wire (purely cosmetic for
    /// the stub; the stub itself doesn't read this).
    pub default_model: String,
}

impl StubProfile {
    /// Conventional `stub` entry with a `stub-model` default.
    pub fn text_only() -> Self {
        Self {
            name: "stub".to_string(),
            default_model: "stub-model".to_string(),
        }
    }
}
