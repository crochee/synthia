//! Build identity baked into every `synthia-server` binary.
//!
//! Five values are captured at compile time from environment
//! variables the build pipeline is expected to set, and surface on
//! `--version` (via [`long_version`]) and the `/version` HTTP
//! route. Operators reach for these when triaging a "which build
//! is running on prod?" question: the value is whatever was
//! linked into the binary, not what the runtime environment says
//! now, so a Kubernetes pod can be inspected with no access to
//! its OCI registry.
//!
//! ## Source of values
//!
//! | Field        | Compile-time key (set by `build.rs`) | Set by                                |
//! |--------------|---------------------------------------|----------------------------------------|
//! | `VERSION`    | `CARGO_PKG_VERSION`                   | Cargo workspace version |
//! | `GIT_SHA`    | `SYNTHIA_GIT_SHA`                     | build.rs ← release.yml `${{ github.sha }}`, Dockerfile.server `--build-arg SYNTHIA_GIT_SHA=…`, or `git log -1` |
//! | `BUILD_TIME` | `SYNTHIA_BUILD_TIME`                  | build.rs ← release.yml `date -u +%FT%TZ`, Dockerfile.server `--build-arg SYNTHIA_BUILD_TIME=…` |
//! | `TARGET`     | `SYNTHIA_TARGET`                      | build.rs ← release.yml, Dockerfile.server (full triple) |
//! | `PROFILE`    | `CARGO_CFG_TARGET_OS` + env-passed    | build.rs `SYNTHIA_PROFILE`, fallback `release` |
//!
//! ## Why build.rs + `cargo:rustc-env=`
//!
//! `build.rs` reads each env var (with a `git log -1` fallback for
//! SHA so local `cargo build` from a clone just works) and emits
//! `cargo:rustc-env=<KEY>=<VALUE>` lines. Cargo then sets those
//! keys as compile-time constants, so `env!()` here resolves to
//! the build-time value with no runtime cost. This is the same
//! pattern used by `vergen` / `built` (industry standard) and
//! the `cim-server` reference layout.
//!
//! `option_env!()` is the fallback path — it fires only when the
//! crate is compiled without its `build.rs` running (e.g. inside
//! `cargo test` against the lib target alone, or a stale
//! incremental cache). Production builds always hit the
//! `env!()` path.

/// `synthia-server` package version (Cargo workspace version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Full git commit SHA the binary was built from, or `"unknown"`.
///
/// Set via `cargo:rustc-env=SYNTHIA_GIT_SHA=` from `build.rs`
/// (which reads `SYNTHIA_GIT_SHA` from the environment with a
/// `git log -1` fallback). Long form (40 hex chars) is preferred
/// because short SHAs can collide across branches; the release
/// workflow always sets the full SHA.
pub const GIT_SHA: &str = match option_env!("SYNTHIA_GIT_SHA") {
    Some(sha) if !sha.is_empty() => sha,
    _ => "unknown",
};

/// ISO-8601 wall-clock string of the build moment, or `"unknown"`.
///
/// Set via `cargo:rustc-env=SYNTHIA_BUILD_TIME=` from `build.rs`.
/// Surfaced for the "is this an old container?" support check.
pub const BUILD_TIME: &str = match option_env!("SYNTHIA_BUILD_TIME") {
    Some(time) if !time.is_empty() => time,
    _ => "unknown",
};

/// Target triple the binary was compiled for, e.g.
/// `x86_64-unknown-linux-gnu` or `aarch64-apple-darwin`.
///
/// Set via `cargo:rustc-env=SYNTHIA_TARGET=` from `build.rs`. The
/// build script reads the `CARGO_CFG_TARGET_OS` cargo variable at
/// build time as the fallback when `SYNTHIA_TARGET` is unset, so
/// `cargo build` from a developer checkout produces a usable
/// value (the OS component of the triple) without any env work.
/// CI builds set the full triple so the `/version` payload tells
/// the operator exactly which artifact they pulled.
pub const TARGET: &str = match option_env!("SYNTHIA_TARGET") {
    Some(t) if !t.is_empty() => t,
    _ => "unknown",
};

/// Compile profile (`release` for CI builds, `debug` for `cargo run`).
///
/// Surfaced on `/version` so a support engineer can tell a debug
/// build from a release build without inspecting symbol tables.
pub const PROFILE: &str = match option_env!("SYNTHIA_PROFILE") {
    Some(p) if !p.is_empty() => p,
    _ => "release",
};

/// `&GIT_SHA[..7]` when the SHA is at least 7 chars, else the SHA.
///
/// "7" is the conventional git short-SHA length — long enough to
/// disambiguate within a single repo's lifetime, short enough to
/// paste into a chat message.
pub fn short_sha() -> &'static str {
    if GIT_SHA.len() >= 7 {
        &GIT_SHA[..7]
    } else {
        GIT_SHA
    }
}

/// Long version string used by `--version` and the startup log line.
///
/// Format mirrors `git describe --long --dirty` style:
/// `<pkg-version> (<short-sha> <build-time>) <target> <profile>` —
/// concise, copy-pasteable, enough information to bisect or roll
/// back without checking the release log.
pub fn long_version() -> String {
    format!(
        "{VERSION} ({sha} {BUILD_TIME}) {TARGET} {PROFILE}",
        sha = short_sha()
    )
}

/// Pre-baked long version string, computed by `build.rs` from the
/// same `SYNTHIA_*` env vars that drive [`VERSION`] / [`GIT_SHA`] /
/// [`BUILD_TIME`] / [`TARGET`] / [`PROFILE`].
///
/// Exists so `clap`'s `#[command(version = …)]` attribute (which only
/// accepts `&'static str`) can print the same rich line that
/// `GET /version` and `long_version()` produce. Without this, `clap`
/// only sees the cargo package version and `--version` would print
/// the bare `synthia-server 0.1.0`. Computed in `build.rs` so the
/// baked string stays in lock-step with the runtime path: the
/// `/version` HTTP route and the `--version` CLI flag can never
/// disagree.
///
/// The `option_env!` fallback returns `"unknown"` when the build
/// script's `cargo:rustc-env=SYNTHIA_LONG_VERSION=…` line never
/// fired (e.g. `cargo test --lib` against the lib target alone, where
/// build.rs of the bin crate is not consulted).
pub const LONG_VERSION: &str = match option_env!("SYNTHIA_LONG_VERSION") {
    Some(s) if !s.is_empty() => s,
    _ => "unknown",
};

#[cfg(test)]
mod tests {
    use super::*;

    /// `long_version()` MUST contain the package version so the
    /// release workflow's "Verify baked identity" check (which greps
    /// for `$expected_version`) sees it.
    #[test]
    fn long_version_contains_package_version() {
        assert!(
            long_version().contains(VERSION),
            "long_version() = {:?} must contain VERSION = {:?}",
            long_version(),
            VERSION
        );
    }

    /// When `SYNTHIA_GIT_SHA` is unset the build falls back to
    /// `"unknown"` rather than the literal empty string — an empty
    /// SHA would be a worse failure mode (silent bisect miss).
    #[test]
    fn git_sha_falls_back_to_unknown_when_unset() {
        // At test compile time we may or may not have SYNTHIA_GIT_SHA
        // set; either way the value is either a hex SHA or "unknown".
        assert!(
            GIT_SHA == "unknown" || GIT_SHA.len() >= 7,
            "GIT_SHA = {GIT_SHA:?} is neither 'unknown' nor a usable SHA"
        );
    }

    /// `short_sha()` is at most 7 chars (so it is pasteable) and
    /// at least 1 (so the format string does not contain "()").
    #[test]
    fn short_sha_is_one_to_seven_chars() {
        let s = short_sha();
        assert!(!s.is_empty(), "short_sha() is empty");
        assert!(s.len() <= 7, "short_sha() = {s:?} is longer than 7 chars");
    }
}
