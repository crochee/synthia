#!/usr/bin/env node
/**
 * Synthia 双侧契约对齐检查（CI 闸门）
 * 退出码:
 *   - 0 双侧完全对齐
 *   - 1 有 frontend-only 或 backend-only
 *   - 2 输入错误（contract 文件不存在等）
 */
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFileSync, existsSync } from 'node:fs';
import { parse as parseYaml } from 'yaml';

import { scanBackendRouter } from './lib/backend-scanner.js';
import { scanFrontendDir } from './lib/frontend-scanner.js';
import { checkContract } from './lib/unifier.js';
import type { ContractFile, Endpoint } from './lib/types.js';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const BACKEND_ROUTER = resolve(
  ROOT,
  process.env.BACKEND_ROUTER ?? 'crates/synthia-server/src/server/router.rs',
);
const FRONTEND_ROOT = resolve(
  ROOT,
  process.env.FRONTEND_ROOT ?? 'synthia-web/src',
);
const IN_YAML = resolve(
  ROOT,
  process.env.IN_YAML ?? 'docs/interface-contract/contract.yaml',
);

/**
 * Load manually-curated entries from the existing `contract.yaml`.
 * Mirrors the logic in `contract-scan.ts` so `contract-check` and
 * `contract-scan` see the same set of endpoints (i.e. the manually
 * preserved fix-card entries don't disappear in CI).
 */
function loadPreserved(path: string): Endpoint[] {
  if (!existsSync(path)) return [];
  let raw: string;
  try {
    raw = readFileSync(path, 'utf8');
  } catch {
    return [];
  }
  const doc = parseYaml(raw) as ContractFile | null;
  if (!doc || !Array.isArray(doc.endpoints)) return [];
  return doc.endpoints;
}

function main() {
  const preserved = loadPreserved(IN_YAML);
  const be = scanBackendRouter(BACKEND_ROUTER);
  const fe = scanFrontendDir(FRONTEND_ROOT);
  const res = checkContract(be, fe, preserved);

  console.log(
    `[contract-check] total=${res.total_endpoints} paired=${res.paired} ` +
      `preserved=${preserved.length} ` +
      `frontend_only=${res.frontend_only.length} backend_only=${res.backend_only.length}`,
  );

  if (res.frontend_only.length) {
    console.error('frontend-only (调用了后端未注册的端点):');
    for (const d of res.frontend_only) {
      for (const ev of d.evidence) console.error(`  ${d.method} ${d.path}   ${ev.file}:${ev.line}`);
    }
  }
  if (res.backend_only.length) {
    console.error('backend-only (后端提供但前端未发现调用):');
    for (const d of res.backend_only) {
      for (const ev of d.evidence) console.error(`  ${d.method} ${d.path}   ${ev.file}:${ev.line}`);
    }
  }

  if (!res.ok) process.exit(1);
}

main();
