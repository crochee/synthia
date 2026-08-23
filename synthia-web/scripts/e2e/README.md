# Frontend E2E / Smoke Tests

These scripts drive the running frontend (Vite dev server) with
a headless Chromium to verify end-to-end behavior. They are NOT
unit tests — they exercise the actual browser, the actual Vite
proxy, and the actual backend on the other side.

## Prerequisites

- Vite dev server running with `SYNTHIA_BACKEND_PORT=8081`
  (or whatever port your backend is bound to):
  ```bash
  cd synthia-web
  SYNTHIA_BACKEND_PORT=8081 npx vite --port 5179 --host 127.0.0.1
  ```
- `synthia-server` running on the matching port:
  ```bash
  cargo run --bin synthia-server -- --port 8081
  ```

## Scripts

### `verify_e2e.mjs` ⭐ — main verification

Drives the full chat flow and validates that consecutive
user messages stay within the same Synthia session across
multiple chat rounds.

```bash
node scripts/e2e/verify_e2e.mjs
```

Expected output ends with `✅ ALL CHECKS PASSED`.

### `inspect_envelope.mjs`

Captures the raw REST envelope sent to `/api/v1/chat/stream` so you can
inspect the wire shape. Useful when debugging request/
response mismatches.

### `check_chat_session.mjs`

Logs every URL navigation in the chat page. Used to verify
the React StrictMode session-creation fix (only one uuid
generated, not two).

### `check_chat_e2e.mjs`

Earlier version of the e2e flow; kept for reference but
superseded by `verify_e2e.mjs`.

### `check_page.mjs`

Sanity check: opens the dev server URL, captures all
console errors and the rendered `#root` inner HTML. Useful
to confirm there is no white-screen regression.

## Browser

Scripts use Playwright's Chromium at
`/home/crochee/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome`.
If your Playwright install uses a different version, edit the
`executablePath` at the top of each script.
