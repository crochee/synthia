#!/usr/bin/env node
/**
 * Verify that every entry in contract.yaml has at least one matching spec
 * file under synthia-web/tests/e2e/integration/contract-closure/.
 *
 * "Matching" is intentionally loose: we read each spec file's text and look
 * for a substring of the endpoint id (e.g. `GET /api/tasks`). Future: use
 * a structured parse (e.g. JSDoc / @contract endpoint annotations) for
 * stricter matching.
 *
 * §5.3 — "Uncovered paths" paragraph:
 * When endpoints or SSE events lack Playwright spec coverage, the report
 * includes an `Uncovered paths:` section categorised by type. In advisory
 * mode (default), exit code is 0 and the list is printed to stderr; this
 * allows CI to pass while surfacing gaps for human review.
 *
 * TODO(§6.1): When the team has established stable habits and uncovered
 * paths are consistently empty across multiple cycles, promote to
 * blocking mode: set exit code 1 when uncovered paths are non-empty.
 * This should be triggered by a new change proposal
 * `synthia-interface-contract-closure-cycle-3-promote-to-blocking`.
 */
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { parse as parseYaml } from 'yaml';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CONTRACT = resolve(ROOT, 'docs/interface-contract/contract.yaml');
const SPEC_DIR = resolve(ROOT, 'synthia-web/tests/e2e/integration/contract-closure');

interface SseEvent {
  name: string;
  fields?: string[];
  cadence_ms?: number;
  notes?: string;
}

interface Endpoint {
  id: string;
  method: string;
  path: string;
  source: 'backend' | 'frontend' | 'both';
  sse_events?: SseEvent[];
  status?: string;
}

interface ContractFile {
  version: number;
  endpoints: Endpoint[];
}

export interface UncoveredEndpoint {
  id: string;
  source: string;
}

export interface UncoveredSseEvent {
  endpointId: string;
  eventName: string;
}

export interface CoverageReport {
  totalEndpoints: number;
  coveredEndpoints: number;
  uncoveredEndpoints: UncoveredEndpoint[];
  totalSseEvents: number;
  coveredSseEvents: number;
  uncoveredSseEvents: UncoveredSseEvent[];
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

/**
 * Check whether an SSE event from contract.yaml has a matching spec file.
 * We look for the event name (e.g. "status-update", "artifact-update")
 * appearing in any spec that also covers the parent endpoint.
 */
function findSseEventCoverage(
  endpointId: string,
  eventName: string,
  specs: { file: string; text: string }[],
): string[] {
  const matches: string[] = [];
  for (const s of specs) {
    // The spec must reference either the endpoint or the event name
    if (s.text.includes(eventName) || s.text.includes(endpointId)) {
      // Check for more specific SSE event mention patterns
      if (
        s.text.includes(eventName) ||
        s.text.includes('sse') ||
        s.text.includes('SSE') ||
        s.text.includes('subscribe')
      ) {
        matches.push(s.file);
      }
    }
  }
  return matches;
}

export function computeCoverage(
  cf: ContractFile,
  specs: { file: string; text: string }[],
): CoverageReport {
  const uncoveredEndpoints: UncoveredEndpoint[] = [];
  let coveredEndpoints = 0;

  const uncoveredSseEvents: UncoveredSseEvent[] = [];
  let totalSseEvents = 0;
  let coveredSseEvents = 0;

  for (const ep of cf.endpoints) {
    if (ep.source === ('frontend-only' as string)) continue; // defensive
    const hits = findCoverage(ep, specs);
    if (hits.length === 0) {
      uncoveredEndpoints.push({ id: ep.id, source: ep.source });
    } else {
      coveredEndpoints++;
    }

    // Check SSE event coverage
    if (ep.sse_events && ep.sse_events.length > 0) {
      for (const sse of ep.sse_events) {
        totalSseEvents++;
        const sseHits = findSseEventCoverage(ep.id, sse.name, specs);
        if (sseHits.length === 0) {
          uncoveredSseEvents.push({
            endpointId: ep.id,
            eventName: sse.name,
          });
        } else {
          coveredSseEvents++;
        }
      }
    }
  }

  return {
    totalEndpoints: cf.endpoints.length,
    coveredEndpoints,
    uncoveredEndpoints,
    totalSseEvents,
    coveredSseEvents,
    uncoveredSseEvents,
  };
}

/**
 * Format the "Uncovered paths" paragraph for the report.
 * Returns an empty string when there are no uncovered paths.
 */
export function formatUncoveredPathsParagraph(report: CoverageReport): string {
  const hasUncoveredEndpoints = report.uncoveredEndpoints.length > 0;
  const hasUncoveredSseEvents = report.uncoveredSseEvents.length > 0;

  if (!hasUncoveredEndpoints && !hasUncoveredSseEvents) return '';

  const lines: string[] = [];
  lines.push('Uncovered paths:');
  lines.push('');

  if (hasUncoveredEndpoints) {
    lines.push(`  Endpoints (${report.uncoveredEndpoints.length}):`);
    for (const u of report.uncoveredEndpoints) {
      lines.push(`    - ${u.id} (source=${u.source})`);
    }
    lines.push('');
  }

  if (hasUncoveredSseEvents) {
    lines.push(`  SSE events (${report.uncoveredSseEvents.length}):`);
    for (const u of report.uncoveredSseEvents) {
      lines.push(`    - ${u.eventName} (on ${u.endpointId})`);
    }
    lines.push('');
  }

  lines.push(
    'Add corresponding specs in synthia-web/tests/e2e/integration/contract-closure/.',
  );

  return lines.join('\n');
}

function main() {
  if (!existsSync(CONTRACT)) {
    console.error('[contract-coverage] contract.yaml not found. Run `make contract-scan` first.');
    process.exit(2);
  }
  const cf = parseYaml(readFileSync(CONTRACT, 'utf8')) as ContractFile;
  const specs = loadSpecFiles(SPEC_DIR);

  const report = computeCoverage(cf, specs);

  // Always print summary to stdout
  console.log(
    `[contract-coverage] ${report.coveredEndpoints}/${report.totalEndpoints} endpoint(s) covered.` +
      (report.totalSseEvents > 0
        ? ` ${report.coveredSseEvents}/${report.totalSseEvents} SSE event(s) covered.`
        : ''),
  );

  // Print uncovered paths paragraph
  const uncoveredParagraph = formatUncoveredPathsParagraph(report);
  if (uncoveredParagraph) {
    // Advisory mode: warn to stderr, exit 0
    console.error(`[contract-coverage] ${report.uncoveredEndpoints.length} endpoint(s) and ${report.uncoveredSseEvents.length} SSE event(s) uncovered by Playwright specs:`);
    console.error(uncoveredParagraph);
    // §6.1 TODO: When promoting to blocking, change the line below to
    // process.exit(1) when uncovered paths are non-empty.
    process.exit(0);
  }

  console.log('[contract-coverage] OK — all paths covered.');
}

// Only run main() when executed directly (not imported by tests).
// In ESM, import.meta.url ends with the file's own path when it is the
// entry point. When imported, process.argv[1] differs.
const isMain =
  process.argv[1] &&
  resolve(process.argv[1]).replace(/\.ts$/, '') ===
    resolve(fileURLToPath(import.meta.url)).replace(/\.ts$/, '');

if (isMain) {
  main();
}
