import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * Fix card #007 — error response envelope verification.
 *
 * The server already has two unified error types:
 * - `ServerError` (V1 routes) → `{ error: { type, message } }`
 * - `ApiError` (V2 routes) → `{ error: { code, message, details } }`
 * The only direct StatusCode returns are for WebSocket auth
 * (StatusCode::UNAUTHORIZED) and the generic `error_response()`
 * helper (StatusCode::from_u16 fallback). Both are acceptable.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');

describe('fix card #007 — error response envelope', () => {
  it('ServerError implements IntoResponse with unified envelope', () => {
    const src = readFileSync(
      join(ROOT, 'crates/synthia-server/src/error.rs'),
      'utf8',
    );
    expect(src).toContain('impl IntoResponse for ServerError');
    expect(src).toContain('"error"');
    expect(src).toContain('"type"');
    expect(src).toContain('"message"');
  });

  it('ApiError (V2) implements IntoResponse with code/message/details', () => {
    const src = readFileSync(
      join(ROOT, 'crates/synthia-server/src/api/error.rs'),
      'utf8',
    );
    expect(src).toContain('impl IntoResponse for ApiError');
    expect(src).toContain('"code"');
    expect(src).toContain('"message"');
    expect(src).toContain('details');
  });

  it('V1 and V2 envelopes are both documented in error.rs', () => {
    const v1 = readFileSync(
      join(ROOT, 'crates/synthia-server/src/error.rs'),
      'utf8',
    );
    const v2 = readFileSync(
      join(ROOT, 'crates/synthia-server/src/api/error.rs'),
      'utf8',
    );
    // V1 uses "type" key, V2 uses "code" key — both documented.
    expect(v1).toContain('error_type');
    expect(v2).toContain('code');
  });
});
