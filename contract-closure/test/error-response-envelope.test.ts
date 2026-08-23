import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Error response envelope verification.
 *
 * The server defines no error variants of its own: every error is
 * a `synthia_core::Error`, and `synthia-server` only maps it to
 * HTTP. `crates/synthia-server/src/api/error.rs` owns:
 * - `AppError` — the boundary struct handlers return
 * - the `From<synthia_core::Error> for AppError` impl owns the
 *   variant → StatusCode mapping inline (no helper table, no
 *   intermediate classifier).
 * - the flat envelope `{ "code", "message" }` where `code` is
 *   the core variant's stable snake_case wire name (错误码) and
 *   `message` is the full thiserror-generated Display output
 *   (`err.to_string()`). The source chain never crosses the
 *   wire; it is logged via `tracing::error!` instead.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');

describe('error response envelope', () => {
  it('AppError implements IntoResponse wrapping synthia_core::Error', () => {
    const src = readFileSync(
      join(ROOT, 'crates/synthia-server/src/api/error.rs'),
      'utf8',
    );
    expect(src).toContain('pub struct AppError');
    expect(src).toMatch(/impl From<(\w+::)?Error> for AppError/);
    expect(src).toMatch(/impl From<std::io::Error> for AppError/);
  });

  it('error code comes from the core wire name, not a server enum', () => {
    const src = readFileSync(
      join(ROOT, 'crates/synthia-server/src/api/error.rs'),
      'utf8',
    );
    // Envelope carries the core-owned snake_case code + full
    // Display message (the axum error-handling pattern).
    expect(src).toMatch(/"code":\s*self\.code/);
    expect(src).toMatch(/"message":\s*self\.message/);
    // No server-owned classifier enum remains.
    expect(src).not.toContain('enum ErrorCode');
    expect(src).not.toContain('struct UserError');
  });

  it('rate-limited errors surface a Retry-After header', () => {
    const src = readFileSync(
      join(ROOT, 'crates/synthia-server/src/api/error.rs'),
      'utf8',
    );
    expect(src).toContain('retry-after');
  });

  it('the legacy V1 ServerError module is gone', () => {
    const libSrc = readFileSync(
      join(ROOT, 'crates/synthia-server/src/lib.rs'),
      'utf8',
    );
    expect(libSrc).not.toContain('pub mod error;');
    expect(libSrc).not.toMatch(/ServerError/);
  });

  it('core does not export the ErrorKind classifier', () => {
    const errSrc = readFileSync(
      join(ROOT, 'crates/synthia-core/src/error.rs'),
      'utf8',
    );
    // No `pub enum ErrorKind` declared anywhere (the classifier
    // was deleted in the 2026-08-23 thiserror migration; the
    // server inlines the wire code per-variant in
    // `From<synthia_core::Error> for AppError`).
    expect(errSrc).not.toMatch(/\bpub\s+enum\s+ErrorKind\b/);
    expect(errSrc).not.toMatch(/\bpub\s+struct\s+ParseErrorKind\b/);
  });
  it('router fallback returns the standard envelope for unknown routes', () => {
    // Without a fallback handler, axum returns a 404 with an
    // empty body. The front-end `ApiClient.toError` parser
    // expects the flat `{"code","message"}` envelope, so we
    // install a `not_found_handler` that wraps the canonical
    // `Error::not_found` and produces the same wire shape.
    const src = readFileSync(
      join(ROOT, 'crates/synthia-server/src/server/router.rs'),
      'utf8',
    );
    expect(src).toMatch(/\.fallback\(not_found_handler\)/);
  });
});

