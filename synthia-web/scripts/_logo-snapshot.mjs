// Throwaway script: render the SVG assets in chromium and write PNG
// snapshots under test-results/ so the visual outcome of the logo
// work can be inspected without spinning up the full dev server.
//
// Each target is wrapped in a minimal HTML document with an `<img>`
// pointing at the SVG — that's how every real consumer (the React
// Header, the GitHub README, an email client) embeds it, so this
// matches the actual rendering surface instead of relying on
// Chromium's standalone SVG viewer (which has its own quirks around
// <text> baseline and viewBox stretching).
import { chromium } from 'playwright';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const outDir = resolve(root, 'test-results', 'logo');
mkdirSync(outDir, { recursive: true });

const targets = [
  { name: 'logo-light', file: 'logo.svg', bg: '#fafafa', color: '#18181b' },
  { name: 'logo-mark-light', file: 'logo-mark.svg', bg: '#fafafa', color: '#18181b' },
  { name: 'logo-mono-light', file: 'logo-mono.svg', bg: '#ffffff', color: '#18181b' },
  { name: 'logo-dark', file: 'logo-inverse.svg', bg: '#0b0d12', color: '#e6e8ee' },
  { name: 'logo-mark-dark', file: 'logo-mark.svg', bg: '#0b0d12', color: '#e6e8ee' },
  { name: 'favicon', file: 'favicon.svg', bg: '#fafafa', color: '#18181b' },
];

const browser = await chromium.launch({
  chromiumSandbox: false,
  args: ['--no-sandbox', '--disable-dev-shm-usage'],
});
const context = await browser.newContext({
  viewport: { width: 800, height: 200 },
  deviceScaleFactor: 2,
});
const page = await context.newPage();

for (const t of targets) {
  // SVG fetched as a sibling file:// resource is the same path the
  // React Header takes (`<img src="/logo.svg">`). Render it via
  // data: URL host so file:// -> file:// from about:blank is allowed.
  const svgPath = resolve(root, 'public', t.file);
  const svgData = readFileSync(svgPath);
  const dataUrl = `data:image/svg+xml;base64,${svgData.toString('base64')}`;
  await page.setViewportSize({ width: 800, height: 200 });
  await page.setContent(
    `<!doctype html><html><head><style>
      html, body { margin:0; padding:24px; background:${t.bg}; color:${t.color};
                   font-family: 'Inter var','Inter', system-ui, 'Helvetica Neue', Arial, sans-serif; }
      .holder { width: 400px; height: 88px; display: block; color: inherit; }
    </style></head><body>
      <img class="holder" src="${dataUrl}" alt="logo">
    </body></html>`,
    { waitUntil: 'networkidle' },
  );
  const buf = await page.locator('img').first().screenshot({ omitBackground: false });
  const out = resolve(outDir, `${t.name}.png`);
  writeFileSync(out, buf);
  console.log(`wrote ${out}`);
}

await browser.close();
