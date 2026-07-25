#!/usr/bin/env node
/**
 * Verify that every entry in contract.yaml has at least one matching spec
 * file under synthia-web/tests/e2e/integration/contract-closure/.
 *
 * "Matching" is intentionally loose: we read each spec file's text and look
 * for a substring of the endpoint id (e.g. `GET /api/tasks`). Future: use
 * a structured parse (e.g. JSDoc / @contract endpoint annotations) for
 * stricter matching.
 */
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { parse as parseYaml } from 'yaml';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CONTRACT = resolve(ROOT, 'docs/interface-contract/contract.yaml');
const SPEC_DIR = resolve(ROOT, 'synthia-web/tests/e2e/integration/contract-closure');

interface Endpoint {
  id: string;
  method: string;
  path: string;
  source: 'backend' | 'frontend' | 'both';
}
interface ContractFile {
  version: number;
  endpoints: Endpoint[];
}

function loadSpecFiles(dir: string): { file: string; text: string }[] {
  if (!existsSync(dir)) return [];
  const out: { file: string; text: string }[] = [];
  for (const name of readdirSync(dir)) {
    if (name.startsWith('_')) continue;
    const full = resolve(dir, name);
    if (!full.endsWith('.spec.ts')) continue;
    out.push({ file: full, text: readFileSync(full, 'utf8') });
  }
  return out;
}

function findCoverage(ep: Endpoint, specs: { file: string; text: string }[]): string[] {
  const matches: string[] = [];
  for (const s of specs) {
    // Naive: the endpoint id (e.g. "GET /api/tasks") must appear as a substring in the spec.
    if (s.text.includes(ep.id)) matches.push(s.file);
  }
  return matches;
}

function main() {
  if (!existsSync(CONTRACT)) {
    console.error('[contract-coverage] contract.yaml not found. Run `make contract-scan` first.');
    process.exit(2);
  }
  const cf = parseYaml(readFileSync(CONTRACT, 'utf8')) as ContractFile;
  const specs = loadSpecFiles(SPEC_DIR);

  const uncovered: { id: string; source: string }[] = [];
  for (const ep of cf.endpoints) {
    if (ep.source === 'frontend-only' as string) continue; // not a registered source value; defensive
    const hits = findCoverage(ep, specs);
    if (hits.length === 0) uncovered.push({ id: ep.id, source: ep.source });
  }

  if (uncovered.length > 0) {
    console.error(`[contract-coverage] ${uncovered.length} endpoint(s) uncovered by Playwright specs:`);
    for (const u of uncovered) console.error(`  - ${u.id} (source=${u.source})`);
    console.error('Add a corresponding spec in synthia-web/tests/e2e/integration/contract-closure/.');
    process.exit(1);
  }

  console.log(`[contract-coverage] OK — ${cf.endpoints.length} endpoint(s) all covered.`);
}

main();
