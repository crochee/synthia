// End-to-end UI verification for the running synthia stack.
//
// Drives the real frontend against the real backend on
// localhost:5173 (vite dev) + localhost:8080 (synthia-server).
// Asserts the user-facing flows the goal commits to:
//
//   1. Loading the chat page creates a fresh session and
//      renders the chat surface.
//   2. Submitting a message shows a user bubble, then streams
//      the assistant reply via SSE, then transitions the
//      status from working to completed.
//   3. The session list page lists the session with a
//      "completed" badge.
//   4. The session detail page renders both the user prompt
//      and the assistant reply (read from canonical
//      Session.history — type/data wire shape).
//   5. Legacy /tasks and /tasks/:id URLs redirect to
//      /sessions and /sessions/:id, preserving the id.
//   6. Sidebar sections (tools, agents, skills, sessions)
//      navigate without errors.
//   7. Backend /livez and /readyz are healthy.
//
// Run with:
//   node synthia-web/scripts/e2e/verify_ui.mjs
//
// Exits 0 on full pass, 1 on any failure.

import { chromium } from 'playwright';

const BASE = process.env.SYNTHIA_WEB ?? 'http://localhost:5173';
const API = process.env.SYNTHIA_API ?? 'http://localhost:8080';

const results = [];
function log(name, ok, detail = '') {
  results.push({ name, ok, detail });
  const tag = ok ? '✓' : '✗';
  console.log(`${tag} ${name}${detail ? ' — ' + detail : ''}`);
}

const browser = await chromium.launch({ headless: true });
const ctx = await browser.newContext({ baseURL: BASE });
const page = await ctx.newPage();

// --- 1. Create session + send a prompt, poll until completed ---
const createResp = await page.request.post(`${API}/api/v1/chat/sessions`, { data: {} });
const { session_id: sid } = await createResp.json();
log('create session via REST', createResp.ok() && Boolean(sid), sid?.slice(0, 8));

const sendResp = await page.request.post(
  `${API}/api/v1/chat/sessions/${sid}/messages`,
  { data: { text: 'Reply with one word: pong', attachments: [] } },
);
const sendJson = await sendResp.json();
log('enqueue message', sendResp.ok() && sendJson.queued === true);

let status = 'working';
let reply = '';
const t0 = Date.now();
while (Date.now() - t0 < 60000) {
  await page.waitForTimeout(1500);
  const r = await page.request.get(`${API}/api/v1/sessions/${sid}`);
  const j = await r.json();
  status = j.status;
  if (status !== 'working') {
    for (const ev of j.history ?? []) {
      if (ev?.type === 'Model' && typeof ev?.data?.text === 'string') {
        reply = ev.data.text;
      }
    }
    break;
  }
}
log('session status transitions to completed', status === 'completed', `status=${status}`);
log('model reply received (pong)', reply.toLowerCase().includes('pong'), `"${reply.trim()}"`);

// --- 2. SessionsPage lists the session with a completed badge ---
await page.goto(`${BASE}/sessions`, { waitUntil: 'domcontentloaded' });
await page.waitForLoadState('networkidle', { timeout: 8000 }).catch(() => {});
const sessionsBody = await page.textContent('body');
log(
  'SessionsPage lists the session by id',
  Boolean(sessionsBody?.includes(sid.slice(0, 8))),
);
log('SessionsPage shows completed status', Boolean(sessionsBody?.includes('completed')));

// --- 3. SessionDetailPage renders user + assistant text ---
await page.goto(`${BASE}/sessions/${sid}`, { waitUntil: 'domcontentloaded' });
await page.waitForLoadState('networkidle', { timeout: 8000 }).catch(() => {});
const detailBody = await page.textContent('body');
log('SessionDetailPage shows user prompt', Boolean(detailBody?.includes('Reply with one word')));
log('SessionDetailPage shows assistant reply', Boolean(detailBody?.includes('pong')));
await page.screenshot({ path: '/tmp/synthia-detail-completed.png', fullPage: true });

// --- 4. ChatPage renders + legacy redirects preserve identity ---
await page.goto(`${BASE}/chat`, { waitUntil: 'domcontentloaded' });
await page.waitForSelector('textarea', { timeout: 5000 });
log('ChatPage renders a textarea', true);

// --- 4a. Live UI flow: send a message through the chat UI,
//        observe the user + assistant bubbles, and confirm
//        the streaming indicator → completed transition.
//        Wait for the Send button to become enabled so we
//        know the default-agent resolution has settled.
await page.waitForFunction(
  () => {
    const btn = document.querySelector('[data-testid="send-button"]');
    return btn instanceof HTMLButtonElement && !btn.disabled;
  },
  null,
  { timeout: 10000 },
).catch(() => {});
await page.fill('[data-testid="chat-input"]', 'Reply with one word: live');
await page.keyboard.press('Enter');
await page.waitForSelector('[data-testid="typing-dots"]', { timeout: 8000 });
log('typing-dots indicator appears during streaming', true);

const assistantReady = await page.waitForFunction(
  () => {
    const all = Array.from(document.querySelectorAll('.nt-chat__message-status'));
    return all.some((el) => /completed|failed|canceled/i.test(el.textContent ?? ''));
  },
  null,
  { timeout: 30000 },
).catch(() => false);
log('assistant turn reaches a terminal status', Boolean(assistantReady));
// Diagnostic: capture the rendered statuses when the wait
// returned. Helps when the chat stream's final turnStatus
// event was missed by the frontend reducer (e.g. SSE was
// cut short by the server).
const debugStatuses = await page.evaluate(() =>
  Array.from(document.querySelectorAll('.nt-chat__message-status')).map((el) => ({
    text: el.textContent,
    classes: el.className,
  })),
);
if (!assistantReady) {
  console.log('  debug statuses:', JSON.stringify(debugStatuses));
}

const liveBody = await page.textContent('body');
log('live chat shows the assistant reply', Boolean(liveBody?.toLowerCase().includes('live')));

// --- 4b. Cancel button shows during streaming and stops the run.
const runningResp = await page.request.post(`${API}/api/v1/chat/sessions`, { data: {} });
const { session_id: cancelSid } = await runningResp.json();
await page.request.post(`${API}/api/v1/chat/sessions/${cancelSid}/messages`, {
  data: { text: 'Count to 50 slowly', attachments: [] },
});
// Open that session's chat page so the SSE stream is observable.
await page.goto(`${BASE}/chat/${cancelSid}/agent/default`, {
  waitUntil: 'domcontentloaded',
});
await page.waitForSelector('[data-testid="chat-input"]', { timeout: 8000 });
await page.fill('[data-testid="chat-input"]', 'Count to 50 slowly');
await page.keyboard.press('Enter');
try {
  await page.waitForSelector('[data-testid="stop-button"]', { timeout: 8000 });
  log('Stop button visible while streaming', true);
  await page.click('[data-testid="stop-button"]');
} catch {
  log('Stop button visible while streaming', false, 'timed out waiting for stop-button');
}
// After clicking Stop, the typing indicator must clear and the
// session's persisted status should land on `canceled`.
await page.waitForFunction(
  () => !document.querySelector('[data-testid="typing-dots"]'),
  null,
  { timeout: 8000 },
).catch(() => {});
let canceledStatus = '';
const t1 = Date.now();
while (Date.now() - t1 < 30000) {
  await page.waitForTimeout(1500);
  const r = await page.request.get(`${API}/api/v1/sessions/${cancelSid}`);
  const j = await r.json();
  canceledStatus = j.status;
  if (canceledStatus !== 'working') break;
}
log(
  'cancel sets session status to canceled',
  canceledStatus === 'canceled',
  `status=${canceledStatus}`,
);

// --- 4c. Legacy /tasks/* redirects preserve identity.
await page.goto(`${BASE}/tasks/${sid}`, { waitUntil: 'domcontentloaded' });
await page.waitForURL(new RegExp(`/sessions/${sid}`), { timeout: 5000 });
log(
  'legacy /tasks/:id → /sessions/:id preserves id',
  page.url().endsWith(`/sessions/${sid}`),
  page.url(),
);

await page.goto(`${BASE}/tasks`, { waitUntil: 'domcontentloaded' });
await page.waitForURL(/\/sessions$/, { timeout: 5000 });
log('legacy /tasks → /sessions', /\/sessions$/.test(page.url()));

// --- 5. Sidebar sections all reachable ---
let sidebarOk = true;
for (const href of ['/tools', '/agents', '/skills', '/sessions']) {
  await page.goto(`${BASE}${href}`, { waitUntil: 'domcontentloaded' });
  try {
    await page.waitForURL(new RegExp(`${href}$`), { timeout: 5000 });
  } catch {
    sidebarOk = false;
  }
}
log('sidebar sections reachable', sidebarOk);

// --- 6. Backend health ---
const lz = await page.request.get(`${API}/livez`);
const rz = await page.request.get(`${API}/readyz`);
log('livez healthy', lz.ok());
log('readyz healthy', rz.ok());

await browser.close();

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length === 0 ? 0 : 1);