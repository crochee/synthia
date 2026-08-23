import { chromium } from 'playwright';

const browser = await chromium.launch({
  executablePath: '/home/crochee/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome',
  headless: true,
});
const context = await browser.newContext();
const page = await context.newPage();

const errors = [];
const consoleLogs = [];
page.on('pageerror', (err) => {
  errors.push(`PAGEERROR: ${err.message}\n${err.stack ?? ''}`);
});
page.on('console', (msg) => {
  consoleLogs.push(`[${msg.type()}] ${msg.text()}`);
});
page.on('requestfailed', (req) => {
  errors.push(`REQUEST FAILED: ${req.url()} - ${req.failure()?.errorText ?? 'unknown'}`);
});

try {
  await page.goto('http://127.0.0.1:5175/', { waitUntil: 'networkidle', timeout: 15000 });
  await page.waitForTimeout(2000);
  const rootHtml = await page.evaluate(
    () => document.getElementById('root')?.innerHTML?.substring(0, 500) ?? 'null',
  );
  console.log('=== ROOT INNER HTML (first 500 chars) ===');
  console.log(rootHtml);
  console.log('\n=== PAGE ERRORS ===');
  for (const e of errors) console.log(e);
  console.log('\n=== CONSOLE LOGS ===');
  for (const l of consoleLogs) console.log(l);
} finally {
  await browser.close();
}
