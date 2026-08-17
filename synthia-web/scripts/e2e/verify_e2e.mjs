// Comprehensive e2e: verify the session/task separation fix
// by using the REAL frontend code paths against a REAL backend.
//
// What we test:
//  1. Page load at "/" navigates to "/chat/<uuid>" (StrictMode
//     safe — only one uuid, not two).
//  2. First user message → POST /a2a/message:send with
//     contextId=<uuid>, messageId=<new-uuid>, taskId absent.
//  3. Second user message → POST /a2a/message:send with the
//     SAME contextId but a NEW messageId, taskId still absent.
//  4. Both requests land on the SAME Synthia session (verified
//     by querying the backend's /a2a/tasks for that context
//     and counting tasks — should be 2, both with same
//     contextId).
//  5. Browser URL stays at /chat/<uuid> across both messages
//     (no spurious URL flips).

import { chromium } from 'playwright';

const browser = await chromium.launch({
  executablePath: '/home/crochee/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome',
  headless: true,
});
const ctx = await browser.newContext();
const page = await ctx.newPage();

const sendReqs = [];
const sendResps = [];
const navHistory = [];
page.on('framenavigated', (f) => { if (f === page.mainFrame()) navHistory.push(f.url()); });
page.on('request', (req) => {
  // A2A SDK uses JSON-RPC over `POST /a2a/` for streaming;
  // REST `POST /a2a/message:send` is the alternate transport.
  // Catch both so the test sees every client-to-server dispatch.
  if (/\/a2a(\/|\/message:send|$)/.test(req.url()) && req.method() === 'POST') {
    const body = req.postData();
    sendReqs.push({ url: req.url(), method: req.method(), body });
  }
});
page.on('response', async (resp) => {
  if (/\/a2a(\/|\/message:send|$)/.test(resp.url()) && resp.request().method() === 'POST') {
    let body = '';
    try { body = (await resp.text()).substring(0, 400); } catch {}
    sendResps.push({ status: resp.status(), body });
  }
});

const url0 = 'http://127.0.0.1:5179/';
await page.goto(url0, { waitUntil: 'networkidle', timeout: 30000 });
await page.waitForTimeout(2500);

function getSessionId(u) {
  const m = u.match(/\/chat\/([^/?#]+)/);
  return m ? m[1] : null;
}

const sessionId = getSessionId(page.url());
console.log('=== STAGE 1: page load ===');
console.log('Final URL after load:', page.url());
console.log('Session ID:', sessionId);
console.log('Nav history count (StrictMode check):', navHistory.length);
const chatNavs = navHistory.filter((u) => u.includes('/chat/'));
const distinctSessionIds = new Set(chatNavs.map(getSessionId));
console.log('Distinct chat session IDs across all NAVs:', [...distinctSessionIds]);
console.log();

// Find the chat textarea
const inputs = await page.$$('textarea');
if (inputs.length === 0) {
  console.log('NO TEXTAREA found, page HTML:');
  console.log((await page.content()).substring(0, 1500));
  await browser.close();
  process.exit(1);
}
const chatInput = inputs[0];

// Send message 1
console.log('=== STAGE 2: send message 1 ===');
await chatInput.click({ timeout: 5000 });
await chatInput.fill('Hi, what is 1+1?');
await chatInput.press('Enter');
await page.waitForTimeout(8000); // wait for response

const url1 = page.url();
console.log('URL after msg 1:', url1);
console.log('Session ID after msg 1:', getSessionId(url1));
function extractMsgMeta(req) {
  if (!req?.body) return null;
  let j;
  try { j = JSON.parse(req.body); } catch { return null; }
  // JSON-RPC envelope: { jsonrpc, id, method, params: { ... } }
  // REST envelope: { tenant, message: {...} }
  // The Message can be at j.message or j.params.message.
  const m = j.message ?? j.params?.message ?? null;
  if (!m) return { envelope: j };
  return {
    envelope: j,
    messageId: m.messageId,
    contextId: m.contextId,
    taskId: m.taskId,
    transport: j.method ? 'JSON-RPC' : 'REST',
    rpcMethod: j.method,
  };
}

// Send message 2
console.log('message:send requests so far:', sendReqs.length);
for (let i = 0; i < sendReqs.length; i++) {
  const meta = extractMsgMeta(sendReqs[i]);
  console.log(`  req[${i}] transport=${meta?.transport} method=${meta?.rpcMethod ?? '-'}`);
  console.log(`           messageId=${meta?.messageId?.substring(0, 8)}...`);
  console.log(`           contextId=${meta?.contextId?.substring(0, 18)}...`);
  console.log(`           taskId=${meta?.taskId?.substring(0, 8) ?? '(none)'}...`);
}
console.log();

console.log('=== STAGE 3: send message 2 ===');
// Wait for input to be re-enabled (streaming completed)
await page.waitForTimeout(2000);
await chatInput.click({ timeout: 5000 }).catch(() => {});
await chatInput.fill('And what is 2+2?');
await chatInput.press('Enter');
await page.waitForTimeout(8000);

const url2 = page.url();
console.log('URL after msg 2:', url2);
console.log('Session ID after msg 2:', getSessionId(url2));
console.log('message:send requests so far:', sendReqs.length);
for (let i = 0; i < sendReqs.length; i++) {
  const meta = extractMsgMeta(sendReqs[i]);
  console.log(`  req[${i}] transport=${meta?.transport} method=${meta?.rpcMethod ?? '-'}`);
  console.log(`           messageId=${meta?.messageId?.substring(0, 8)}...`);
  console.log(`           contextId=${meta?.contextId?.substring(0, 18)}...`);
  console.log(`           taskId=${meta?.taskId?.substring(0, 8) ?? '(none)'}...`);
}
console.log();

// Query backend for the tasks of this context
console.log('=== STAGE 4: backend verification ===');
const tasksResp = await page.evaluate(async (cid) => {
  const r = await fetch(`/a2a/tasks?contextId=${encodeURIComponent(cid)}`);
  return { status: r.status, body: await r.text() };
}, sessionId);
const tasksJson = JSON.parse(tasksResp.body);
const tasks = tasksJson.tasks || tasksJson.result?.tasks || [];
console.log(`Backend reports ${tasks.length} task(s) for contextId=${sessionId.substring(0, 18)}...`);
for (const t of tasks) {
  console.log(`  task_id=${t.id?.substring(0, 8)}... contextId=${t.contextId?.substring(0, 18)}... status=${t.status?.state ?? t.status}`);
}
console.log();

// Final verdict
console.log('=== VERDICT ===');
const checks = [
  ['StrictMode: only 1 distinct session id across NAVs', distinctSessionIds.size === 1],
  ['URL stable across message 1', getSessionId(url1) === sessionId],
  ['URL stable across message 2', getSessionId(url2) === sessionId],
  ['Both message:send requests fired', sendReqs.length >= 2],
  ['Both requests use SAME contextId', (() => {
    if (sendReqs.length < 2) return false;
    const c1 = extractMsgMeta(sendReqs[0])?.contextId;
    const c2 = extractMsgMeta(sendReqs[1])?.contextId;
    return c1 === c2 && c1 === sessionId;
  })()],
  ['Both requests have DIFFERENT messageId', (() => {
    if (sendReqs.length < 2) return false;
    const m1 = extractMsgMeta(sendReqs[0])?.messageId;
    const m2 = extractMsgMeta(sendReqs[1])?.messageId;
    return m1 && m2 && m1 !== m2;
  })()],
  ['Frontend uses JSON-RPC transport', (() => {
    if (sendReqs.length < 1) return false;
    return extractMsgMeta(sendReqs[0])?.transport === 'JSON-RPC';
  })()],
  ['Backend records 2 tasks for this context', tasks.length === 2],
  ['Both tasks share the same contextId', tasks.every((t) => t.contextId === sessionId)],
  ['Both task IDs are unique', new Set(tasks.map((t) => t.id)).size === tasks.length],
];
for (const [name, ok] of checks) {
  console.log(`  ${ok ? '✅' : '❌'} ${name}`);
}
const allPass = checks.every(([_, ok]) => ok);
console.log();
console.log(allPass ? '✅ ALL CHECKS PASSED' : '❌ SOME CHECKS FAILED');

await browser.close();
process.exit(allPass ? 0 : 1);