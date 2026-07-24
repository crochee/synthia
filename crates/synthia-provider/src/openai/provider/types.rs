//! The [`TransformOptions`] struct — a placeholder for future
//! per-call options (e.g. `extract_media` toggles). Currently
//! only `_extract_media` is reserved; callers should pass
//! `TransformOptions::default()` to [`super::transform::OpenAICompatibleProvider::transform_message_with_options`].

#[derive(Debug, Default)]
pub struct TransformOptions {
    pub _extract_media: bool,
}
