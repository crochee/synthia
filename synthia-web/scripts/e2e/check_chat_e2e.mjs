// Frontend e2e: open /chat, send 2 messages in same chat,
// verify they go to the same Synthia session (per backend logs)
// and only ONE session_id is created (StrictMode fix still holds).

import { chromium } from 'playwright';

const browser = await chromium.launch({
  executablePath: '/home/crochee/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome',
  headless: true,
});
const ctx = await browser.newContext();
const page = await ctx.newPage();

const logs = [];
const requests = [];
page.on('console', (msg) => logs.push(`[${msg.type()}] ${msg.text().substring(0, 200)}`));
page.on('framenavigated', (f) => {
  if (f === page.mainFrame()) logs.push(`NAV: ${f.url()}`);
});
page.on('request', (req) => {
  if (req.url().includes('/api/v1/chat/message')) {
    let body = '';
    req
      .postData()
      .then((d) => logs.push(`SEND-REQ: ${req.url()} body=${(d ?? '').substring(0, 200)}`));
  }
});
page.on('response', async (resp) => {
  if (resp.url().includes('/api/v1/chat/message')) {
    logs.push(`SEND-RESP: ${resp.status()} ${resp.url()}`);
  }
});

await page.goto('http://127.0.0.1:5178/', { waitUntil: 'networkidle', timeout: 30000 });
await page.waitForTimeout(2500);

const url1 = page.url();
console.log('After load:', url1);

// Look for textarea/input for message sending
const inputs = await page.$$('textarea, input[type="text"]');
console.log(`Found ${inputs.length} text inputs`);

if (inputs.length === 0) {
  console.log('PAGE HTML (first 1500):');
  const html = await page.evaluate(() => document.body.innerHTML.substring(0, 1500));
  console.log(html);
  await browser.close();
  process.exit(0);
}

// Find a textarea that's likely the chat input (the bigger one)
let chatInput = null;
for (const inp of inputs) {
  const tag = await inp.evaluate((el) => el.tagName);
  if (tag === 'TEXTAREA') {
    chatInput = inp;
    break;
  }
}
if (!chatInput) chatInput = inputs[0];

// Print input state
const inputState = await chatInput.evaluate((el) => ({
  tag: el.tagName,
  type: el.type ?? null,
  disabled: el.disabled,
  readOnly: el.readOnly,
  placeholder: el.placeholder ?? null,
  visible: el.offsetParent !== null,
}));
console.log('Chat input state:', inputState);

await chatInput.click({ timeout: 5000 }).catch((e) => console.log('Click failed:', e.message));
await page.waitForTimeout(500);
await chatInput
  .fill('What is 2 + 2?', { timeout: 5000 })
  .catch((e) => console.log('Fill failed:', e.message));
await chatInput.press('Enter').catch((e) => console.log('Enter failed:', e.message));
await page.waitForTimeout(8000); // wait for response

const url2 = page.url();
console.log('After message 1:', url2);

await chatInput.fill('And what is 3 + 3?');
await chatInput.press('Enter');
await page.waitForTimeout(8000); // wait for response

const url3 = page.url();
console.log('After message 2:', url3);

// Extract session_id from URL
function getSessionId(u) {
  const m = u.match(/\/chat\/([^/?#]+)/);
  return m ? m[1] : null;
}

const s1 = getSessionId(url1);
const s2 = getSessionId(url2);
const s3 = getSessionId(url3);

console.log();
console.log('=== VERIFICATION ===');
console.log(`Session ID after load:       ${s1}`);
console.log(`Session ID after message 1:  ${s2}`);
console.log(`Session ID after message 2:  ${s3}`);
console.log();
console.log(`Same session across both messages: ${s1 && s2 === s1 && s3 === s1}`);

await browser.close();
console.log();
console.log('=== Last 30 console logs ===');
for (const l of logs.slice(-30)) console.log(l);
