#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Discovered,
    Connecting,
    Connected,
    Idle,
    Error,
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Discovered => write!(f, "discovered"),
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Connected => write!(f, "connected"),
            ConnectionState::Idle => write!(f, "idle"),
            ConnectionState::Error => write!(f, "error"),
        }
    }
}
