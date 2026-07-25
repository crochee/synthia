#!/usr/bin/env node
/**
 * Synthia 双侧契约扫描器入口
 *
 * 读取:
 *   - BACKEND_ROUTER (默认: crates/synthia-server/src/server/router.rs)
 *   - FRONTEND_GLOB (默认: synthia-web/src)
 *
 * 写出:
 *   - docs/interface-contract/contract.yaml
 *   - docs/interface-contract/contract.json
 */
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { writeFileSync } from 'node:fs';
import { stringify as yamlStringify } from 'yaml';

import { scanBackendRouter } from './lib/backend-scanner.js';
import { scanFrontendDir } from './lib/frontend-scanner.js';
import { unionEndpoints } from './lib/unifier.js';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const BACKEND_ROUTER = resolve(
  ROOT,
  process.env.BACKEND_ROUTER ?? 'crates/synthia-server/src/server/router.rs',
);
const FRONTEND_ROOT = resolve(
  ROOT,
  process.env.FRONTEND_ROOT ?? 'synthia-web/src',
);
const OUT_YAML = resolve(ROOT, 'docs/interface-contract/contract.yaml');
const OUT_JSON = resolve(ROOT, 'docs/interface-contract/contract.json');

function main() {
  const be = scanBackendRouter(BACKEND_ROUTER);
  const fe = scanFrontendDir(FRONTEND_ROOT);
  const cf = unionEndpoints(be, fe);
  writeFileSync(OUT_YAML, yamlStringify(cf), 'utf8');
  writeFileSync(OUT_JSON, JSON.stringify(cf, null, 2), 'utf8');
  console.log(
    `[contract-scan] backend=${be.length} frontend=${fe.length} total=${cf.endpoints.length} -> ${OUT_YAML}`,
  );
}

main();
