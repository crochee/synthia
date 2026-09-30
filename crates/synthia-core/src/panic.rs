//! Panic-payload extraction — one canonical implementation.
//!
//! Four crates catch panics from third-party code (a tool body, a hook,
//! a guard, a provider, a request handler) and each needs the same
//! thing: turn the payload
//! [`catch_unwind`](std::panic::catch_unwind) hands back into a message
//! worth logging or showing. They had four copies and three different
//! fallback strings, so the same panic read differently depending on
//! which layer caught it.
//!
//! ## Why the payload is taken *by value*
//!
//! [`panic_message`] takes `Box<dyn Any + Send>`, not
//! `&Box<..>` or `&dyn Any`. That is deliberate: a `Box<dyn Any + Send>`
//! is itself `Any`, so a reference parameter lets a caller write
//! `&payload`, which un-sizes to `dyn Any` *over the Box* — every
//! `downcast_ref` then misses and the message silently degrades to the
//! fallback. That mistake compiles, produces plausible output, and is
//! invisible without a test asserting the message survives. Requiring
//! ownership makes it unrepresentable at the call site.
//!
//! ```
//! let caught = std::panic::catch_unwind(|| panic!("disk full"));
//! let payload = caught.expect_err("the closure panics");
//! assert_eq!(synthia_core::panic_message(payload), "disk full");
//! ```

/// The message from a caught panic payload.
///
/// A panic payload is `Box<dyn Any + Send>`. `panic!("literal")`
/// and `panic!("{formatted}")` both deliver a `&'static str`; an
/// explicit [`std::panic::panic_any`] with a `String` delivers that.
/// Anything else — a non-string payload from a caller that chose one,
/// or a backend that could not preserve the type — becomes the
/// non-string fallback (`(non-string panic payload)`) rather than being
/// lost.
///
/// # Examples
///
/// ```
/// let payload = std::panic::catch_unwind(|| panic!("boom"))
///     .expect_err("the closure panics");
/// assert_eq!(synthia_core::panic_message(payload), "boom");
/// ```
#[must_use]
pub fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        NON_STRING_PANIC.to_string()
    }
}

/// Fallback used when a payload carries no string.
///
/// Crate-internal: the only reader is [`panic_message`] itself. A caller
/// that needs to recognise the fallback can compare against the returned
/// string; exposing the constant would add public surface with no
/// consumer.
pub(crate) const NON_STRING_PANIC: &str = "(non-string panic payload)";

#[cfg(test)]
mod tests {
    use super::*;

    fn caught(
        f: impl FnOnce() + std::panic::UnwindSafe,
    ) -> Box<dyn std::any::Any + Send> {
        std::panic::catch_unwind(f).expect_err("the closure must panic")
    }

    /// The three shapes a message arrives in — a literal, a formatted
    /// string, and an explicit `panic_any` — all survive verbatim.
    #[test]
    fn string_payloads_keep_their_message() {
        assert_eq!(
            panic_message(caught(|| panic!("literal boom"))),
            "literal boom"
        );
        assert_eq!(
            panic_message(caught(|| panic!("formatted {}", 42))),
            "formatted 42"
        );
        assert_eq!(
            panic_message(caught(|| std::panic::panic_any(String::from(
                "owned boom"
            )))),
            "owned boom"
        );
        assert_eq!(
            panic_message(caught(|| std::panic::panic_any("borrowed boom"))),
            "borrowed boom"
        );
    }

    /// A non-string payload degrades to the named fallback rather than
    /// an empty string, so the log line still says a panic happened.
    #[test]
    fn non_string_payload_uses_the_named_fallback() {
        assert_eq!(
            panic_message(caught(|| std::panic::panic_any(42u32))),
            NON_STRING_PANIC
        );
    }
}
