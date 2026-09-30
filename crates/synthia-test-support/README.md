# synthia-test-support

Shared mock implementations for cross-crate testing: `ModelProviderStub`,
`ReplayProvider`, fixed clocks / sequences, fixture golden-transcripts.

This crate exists **only** to be a dev-dependency of other crates; it
is **not** an API consumer should ever depend on for production code.

## CI 契约

- `cargo test -p synthia-test-support --lib` 绿；
- 不出现在公开 contract 内（不在 facade feature 集合）。
