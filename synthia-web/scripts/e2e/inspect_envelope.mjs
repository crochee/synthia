// Inspect raw JSON-RPC envelope shape
import { chromium } from 'playwright';

const browser = await chromium.launch({
  executablePath: '/home/crochee/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome',
  headless: true,
});
const ctx = await browser.newContext();
const page = await ctx.newPage();

let captured = null;
page.on('request', (req) => {
  if (/\/a2a(\/|\/message:send|$)/.test(req.url()) && req.method() === 'POST') {
    if (captured) return;
    captured = { url: req.url(), body: req.postData() };
  }
});

await page.goto('http://127.0.0.1:5179/', { waitUntil: 'networkidle', timeout: 30000 });
await page.waitForTimeout(2000);
const inputs = await page.$$('textarea');
await inputs[0].click();
await inputs[0].fill('Just a quick smoke test');
await inputs[0].press('Enter');
await page.waitForTimeout(3000);

if (!captured) {
  console.log('NO REQUEST CAPTURED');
} else {
  console.log('URL:', captured.url);
  console.log('BODY:');
  try {
    const j = JSON.parse(captured.body);
    console.log(JSON.stringify(j, null, 2).substring(0, 1500));
  } catch (e) {
    console.log(captured.body);
  }
}

await browser.close();