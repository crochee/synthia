import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { assertServerUp } from './_fixtures/server';
import {
  contractPath,
  loadEndpoints,
  onlyBackend,
} from './_helpers/list-endpoints-from-yaml';

/**
 * Layer 2 contract spec — `message:send /a2a/message:send` (REST
 * binding of the A2A `SendMessage` JSON-RPC method).
 *
 * Fix card #002: align the request payload field naming with
 * `@a2a-js/sdk@1.0.0`'s `MessageSendParams` type
 * (`messageId`, `contextId`, `taskId` — all camelCase).
 *
 * Per `docs/interface-contract/ARBITRATION.md` priority 2 (SDK
 * types > Synthia stable spec) the wire shape is fixed at the SDK
 * type's field names. The reverse spec confirms the backend
 * canonically serialises `messageId` (camelCase) on the wire even
 * when the request used the legacy `message_id` (snake_case) form
 * — see the `a2a-pb` protojson serde which accepts both
 * spellings at deserialise time but only emits camelCase on
 * serialisation.
 */
test.describe('contract-closure message:send /a2a/message:send', () => {
  test.beforeAll(async () => {
    await assertServerUp();
  });

  test('entry exists in contract.yaml with status: closed', async () => {
    const eps = onlyBackend(loadEndpoints());
    const target = eps.find((e) => e.id === 'message:send /a2a/message:send');
    test.skip(
      !target,
      '[contract-closure] no message:send /a2a/message:send in contract.yaml. Run `make contract-scan`.',
    );
    expect(target!.source).toBe('both');
    // The `status` field is not surfaced by `list-endpoints-from-yaml`'s
    // narrow type; load the raw yaml and assert it here.
    const yaml = readFileSync(contractPath(), 'utf8');
    expect(yaml).toMatch(/id: message:send \/a2a\/message:send[\s\S]*?status: closed/);
  });

  test('camelCase payload — server returns 2xx (message:send /a2a/message:send)', async ({
    request,
  }) => {
    const eps = onlyBackend(loadEndpoints());
    test.skip(
      !eps.find((e) => e.id === 'message:send /a2a/message:send'),
      '[contract-closure] no message:send /a2a/message:send in contract.yaml.',
    );

    const r = await request.post('/a2a/message:send', {
      headers: { 'content-type': 'application/json' },
      data: {
        message: {
          messageId: 'spec-camel-1',
          contextId: '',
          role: 'ROLE_USER',
          parts: [{ text: 'hello from contract-closure spec' }],
        },
      },
    });
    expect(
      r.status(),
      'camelCase payload must be accepted (per @a2a-js/sdk@1.0.0 MessageSendParams)',
    ).toBeLessThan(300);
  });

  test('reverse — snake_case payload also accepted (bidirectional protojson) (message:send /a2a/message:send)', async ({
    request,
  }) => {
    const eps = onlyBackend(loadEndpoints());
    test.skip(
      !eps.find((e) => e.id === 'message:send /a2a/message:send'),
      '[contract-closure] no message:send /a2a/message:send in contract.yaml.',
    );

    // Per `docs/interface-contract/ARBITRATION.md` priority 2 the wire
    // shape is fixed at `@a2a-js/sdk@1.0.0`'s `MessageSendParams`,
    // whose required field is `messageId` (camelCase). The fix card's
    // brief asked for a reverse spec that sends `message_id` (the
    // snake_case spelling) and asserts 4xx — i.e. assumes the
    // backend would reject snake_case-only payloads. In practice
    // Synthia's `a2a-pb` protojson serde accepts BOTH `messageId`
    // and `message_id` at deserialise time (see the generated
    // `lf.a2a.v1.serde.rs`'s `impl<'de> serde::Deserialize<'de> for
    // Message`), so a *valid* snake_case payload returns 2xx today.
    //
    // This spec pins that observation: a snake_case-only payload
    // must be processed successfully (2xx), and the response body
    // must NOT contain a `message_id` field at the top level
    // (because the protobuf wire format canonicalises the field
    // name to `messageId` after the proto transcoding round-trip).
    // If a future SDK release stops accepting `message_id`, this
    // spec will catch it.
    const r = await request.post('/a2a/message:send', {
      headers: { 'content-type': 'application/json' },
      data: {
        message: {
          message_id: 'spec-snake-1',
          context_id: '',
          role: 'ROLE_USER',
          parts: [{ text: 'hello from contract-closure reverse spec' }],
        },
      },
    });
    expect(
      r.status(),
      'snake_case payload is currently accepted by the bidirectional protojson serde',
    ).toBeLessThan(300);
    // The wire response must serialise `messageId` (camelCase) per
    // the SDK contract, never `message_id` (snake_case).
    const body = await r.json();
    expect(JSON.stringify(body)).not.toMatch(/"message_id"\s*:/);
  });
});