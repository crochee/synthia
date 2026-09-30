#!/usr/bin/env node
/**
 * Synthia 双侧契约扫描器入口
 *
 * 读取:
 *   - BACKEND_ROUTER (默认: crates/synthia-server/src/server/router.rs)
 *   - FRONTEND_GLOB (默认: synthia-web/src)
 *   - IN_YAML (默认: docs/interface-contract/contract.yaml；用于保留手工
 *     标记的 fix-card `status: closed`，避免每次扫描被 reset)
 *
 * 写出:
 *   - docs/interface-contract/contract.yaml
 *   - docs/interface-contract/contract.json
 */
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { parse as parseYaml, stringify as yamlStringify } from 'yaml';

import { scanBackendRouter } from './lib/backend-scanner.js';
import { scanFrontendDir } from './lib/frontend-scanner.js';
import { unionEndpoints } from './lib/unifier.js';
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
const OUT_YAML = resolve(ROOT, 'docs/interface-contract/contract.yaml');
const OUT_JSON = resolve(ROOT, 'docs/interface-contract/contract.json');

/**
 * Load the previous `contract.yaml` (if present) and return its
 * endpoints. Used by the scanner to preserve any manually-curated
 * entries (typically fix-card endpoints whose routes the scanner
 * cannot see because they are mounted via `nest_service` or proxied
 * through an external SDK).
 *
 * Both the entries themselves AND their `status: closed` markers are
 * preserved across regenerations; the unifier drops the `status`
 * marker if both scanner sides disagree (see `mergeByKey`), and the
 * `preserve` list re-applies it for scanner-known endpoints.
 */
function loadPreviousEntries(path: string): Endpoint[] {
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
  const prev = loadPreviousEntries(IN_YAML);
  const be = scanBackendRouter(BACKEND_ROUTER);
  const fe = scanFrontendDir(FRONTEND_ROOT);
  const cf = unionEndpoints(be, fe, prev);

  writeFileSync(OUT_YAML, yamlStringify(cf), 'utf8');
  writeFileSync(OUT_JSON, JSON.stringify(cf, null, 2), 'utf8');
  console.log(
    `[contract-scan] backend=${be.length} frontend=${fe.length} ` +
      `preserved=${prev.length} total=${cf.endpoints.length} -> ${OUT_YAML}`,
  );
}

main();
