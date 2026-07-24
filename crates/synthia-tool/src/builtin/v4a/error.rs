//! Parse errors returned by [`super::parser::parse_v4a`].

/// Parse error returned by [`parse_v4a`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    MissingBeginMarker,
    MissingEndMarker,
    UnknownOpHeader(String),
    HunkWithoutUpdate,
    HunkOutOfOrder,
    InvalidPath(String),
    EmptyPatch,
    TrailingGarbage(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::MissingBeginMarker => {
                write!(f, "missing '*** Begin Patch' marker")
            }
            ParseError::MissingEndMarker => {
                write!(f, "missing '*** End Patch' marker")
            }
            ParseError::UnknownOpHeader(h) => {
                write!(f, "unknown op header: {}", h)
            }
            ParseError::HunkWithoutUpdate => {
                write!(f, "hunk line outside of Update File block")
            }
            ParseError::HunkOutOfOrder => {
                write!(f, "hunk '*** End of File' before content")
            }
            ParseError::InvalidPath(p) => write!(f, "invalid path: {}", p),
            ParseError::EmptyPatch => {
                write!(f, "patch is empty (no operations)")
            }
            ParseError::TrailingGarbage(extra) => {
                write!(f, "trailing content after '*** End Patch': {}", extra)
            }
        }
    }
}

impl std::error::Error for ParseError {}
