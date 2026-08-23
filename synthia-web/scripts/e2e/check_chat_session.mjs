import { chromium } from 'playwright';

const browser = await chromium.launch({
  executablePath: '/home/crochee/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome',
  headless: true,
});
const context = await browser.newContext();
const page = await context.newPage();

const logs = [];
page.on('console', (msg) => logs.push(`[${msg.type()}] ${msg.text()}`));
page.on('pageerror', (err) => logs.push(`PAGEERROR: ${err.message}`));
page.on('framenavigated', (f) => {
  if (f === page.mainFrame()) logs.push(`NAV: ${f.url()}`);
});

await page.goto('http://127.0.0.1:5176/', { waitUntil: 'networkidle', timeout: 15000 });
await page.waitForTimeout(2000);

// Log URL after first navigation
console.log('URL after first mount:', page.url());

// Wait a bit more for any effect-driven navigation
await page.waitForTimeout(1000);
console.log('URL after wait:', page.url());

await browser.close();
for (const l of logs.slice(-30)) console.log(l);
