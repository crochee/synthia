## 1. Dependencies and Cargo Configuration

- [x] 1.1 Add `landlock` as an optional dependency in `crates/synthia-sandbox/Cargo.toml`.
- [x] 1.2 Verify `cargo check` passes with default features.
- [x] 1.3 Verify `cargo check --features landlock` compiles the new optional dependency path.

## 2. LandlockBackend Implementation

- [x] 2.1 Implement `LandlockBackend::is_available` using the `landlock` crate ABI probe.
- [x] 2.2 Implement `LandlockBackend::select` to return `SandboxAttempt::Landlock` for `Standard`/`Strict` policies when ABI is available.
- [x] 2.3 Implement `SandboxAttempt::Landlock::wrap` to apply Landlock rules in the child process before exec.
- [x] 2.4 Ensure `SandboxPolicy::None` returns `SandboxAttempt::None` and `Custom` returns `Unavailable`.

## 3. CompositeSandboxManager

- [x] 3.1 Create `crates/synthia-sandbox/src/composite.rs` with `CompositeSandboxManager`.
- [x] 3.2 Implement prioritized backend selection (bubblewrap first, then landlock, then unavailable).
- [x] 3.3 Short-circuit `SandboxPolicy::None` to `SandboxAttempt::None`.
- [x] 3.4 Export `CompositeSandboxManager` from `synthia-sandbox` crate root.

## 4. Wiring and Configuration

- [x] 4.1 Replace direct `BubblewrapBackend` usage with `CompositeSandboxManager` in CLI and server configuration paths.
- [x] 4.2 Ensure `CompositeSandboxManager` is only constructed with `LandlockBackend` when the `landlock` feature is enabled.
- [x] 4.3 Verify no regressions in default build paths.

## 5. Testing

- [x] 5.1 Add unit tests for `LandlockBackend::select` policy mapping.
- [x] 5.2 Add unit tests for `CompositeSandboxManager` fallback ordering.
- [x] 5.3 Add integration test for Landlock workspace isolation (read workspace / read outside workspace).
- [x] 5.4 Run `cargo clippy --all-targets --all-features --tests --all` and fix warnings.
- [x] 5.5 Run `cargo +nightly fmt --all` and commit formatting fixes.

## 6. Documentation

- [x] 6.1 Update `crates/synthia-sandbox/README.md` with Landlock feature usage and constraints.
- [x] 6.2 Add inline module documentation for `LandlockBackend` and `CompositeSandboxManager`.
