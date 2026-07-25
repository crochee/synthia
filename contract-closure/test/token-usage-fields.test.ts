import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * Fix card #009 — token usage field alignment.
 *
 * The `@a2a-js/sdk@1.0.0` does not define a token usage type in
 * the A2A protocol. Token usage is a Synthia-specific extension
 * carried as `Part::data({ kind: "usage", input_tokens, output_tokens,
 * cache_read_tokens, cache_creation_tokens })` on the wire.
 *
 * Both backend (Rust) and frontend (TypeScript) use snake_case
 * for these fields because they are not A2A protocol fields but
 * internal metadata. This is consistent with the convention that
 * A2A protocol fields follow the SDK's camelCase convention while
 * Synthia extensions use snake_case.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');

describe('fix card #009 — token usage field alignment', () => {
  it('backend emits usage with snake_case field names', () => {
    const src = readFileSync(
      join(ROOT, 'crates/synthia-a2a/src/mapping.rs'),
      'utf8',
    );
    // The usage event uses snake_case for Synthia extension fields.
    expect(src).toContain('"input_tokens"');
    expect(src).toContain('"output_tokens"');
    expect(src).toContain('"cache_read_tokens"');
  });

  it('usage is emitted as Part::data Message (not A2A protocol field)', () => {
    const src = readFileSync(
      join(ROOT, 'crates/synthia-a2a/src/mapping.rs'),
      'utf8',
    );
    // Usage is carried as a Message with Part::data, not as a
    // TaskStatus or ArtifactUpdate. This is correct because the
    // A2A protocol does not define a usage field.
    expect(src).toContain('"kind": "usage"');
  });

  it('SDK does not define a token usage type', () => {
    // Verify that the A2A SDK does not have a usage type,
    // confirming that token usage is a Synthia extension.
    const sdkTypes = readFileSync(
      join(ROOT, 'synthia-web/node_modules/@a2a-js/sdk/dist/a2a-Ubve0YhO.d.ts'),
      'utf8',
    );
    expect(
      sdkTypes.includes('prompt_tokens') ||
        sdkTypes.includes('promptTokens'),
      'SDK should not define token usage types',
    ).toBe(false);
  });
});
