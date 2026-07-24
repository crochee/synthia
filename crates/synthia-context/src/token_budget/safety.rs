/// Absolute minimum tokens required for the context to be usable.
/// Below this threshold, operations should be rejected outright.
pub const HARD_MIN: usize = 16000;

/// Warning threshold: below this, the context window is getting dangerously
/// small and the caller should be warned.
pub const WARN_BELOW: usize = 32000;

/// Check whether the given available tokens are within safe operating range.
///
/// Returns an error string if below HARD_MIN, a warning string if below
/// WARN_BELOW, or Ok if safe.
pub fn check_context_safety(
    available_tokens: usize,
) -> Result<(), &'static str> {
    if available_tokens < HARD_MIN {
        return Err(
            "Context tokens below hard minimum (16000). Operation rejected. \
             Increase context window size or reduce input.",
        );
    }
    if available_tokens < WARN_BELOW {
        eprintln!(
            "[synthia-context] Warning: available tokens ({}) below recommended minimum ({}). \
             Consider increasing context window size.",
            available_tokens, WARN_BELOW
        );
    }
    Ok(())
}
