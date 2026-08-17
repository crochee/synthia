//! The [`TransformOptions`] struct — a placeholder for future
//! per-call options (e.g. `extract_media` toggles). Currently
//! only `_extract_media` is reserved; callers should pass
//! `TransformOptions::default()` to [`super::transform::OpenAICompatibleProvider::transform_message_with_options`].

#[derive(Debug, Default)]
pub struct TransformOptions {
    pub _extract_media: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `TransformOptions::default()` MUST populate all fields
    /// with their default values (`_extract_media = false`).
    #[test]
    fn default_all_fields_false() {
        let o = TransformOptions::default();
        assert!(!o._extract_media);
    }

    /// `TransformOptions` MUST support direct field
    /// construction (no constructor required).
    #[test]
    fn direct_construction_works() {
        let o = TransformOptions {
            _extract_media: true,
        };
        assert!(o._extract_media);
    }

    /// `TransformOptions` MUST round-trip via `Debug`
    /// (pinned by deriving `Debug`).
    #[test]
    fn supports_debug() {
        let o = TransformOptions {
            _extract_media: true,
        };
        let s = format!("{:?}", o);
        assert!(s.contains("TransformOptions"));
        assert!(s.contains("true"));
    }

    /// `TransformOptions::default()` MUST be deterministic
    /// (two calls produce equal values).
    #[test]
    fn default_is_deterministic() {
        let a = TransformOptions::default();
        let b = TransformOptions::default();
        assert_eq!(a._extract_media, b._extract_media);
    }
}
