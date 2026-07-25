/**
 * 双侧契约合并 + diff
 * - 输入：backend endpoints, frontend endpoints
 * - 输出：ContractFile
 */
import type { CheckResult, ContractFile, Dangling, Endpoint } from './types.js';

/**
 * Normalize a path so that any `{anything}` placeholder collapses to `{key}`,
 * removing differences due to per-endpoint placeholder naming (backend may
 * use `{id}`, `{name}`, `{key}` — they all represent the same dynamic segment).
 */
export function normalizePathKey(s: string): string {
  return s.replace(/\{[A-Za-z_$][\w$]*\}/g, '{key}');
}

function keyOf(e: { method: string; path: string }): string {
  return `${e.method} ${normalizePathKey(e.path)}`;
}

/**
 * Merge two sets of endpoints keyed by `<METHOD> <canonical-path>`.
 *
 * The merge is a *union* (not an intersection): if the backend and
 * frontend disagree on `source_files`, the union keeps both sides.
 * `status` (fix-card lifecycle marker) is preserved only when both
 * sides agree; on disagreement the merged entry drops it (i.e. flips
 * back to open) so CI does not silently keep a stale `closed` flag
 * after a payload-shape regression on one side.
 *
 * `preserve` is an optional list of manually-curated entries (e.g. a
 * fix-card endpoint that the scanner can't see because the route is
 * mounted via `nest_service` or proxied through an external SDK).
 * Each preserved entry is added to the union with `source: 'both'`
 * unless its key collides with a scanner-derived entry (in which case
 * the scanner-derived entry wins and its `status` is left untouched).
 */
function mergeByKey(
  be: Endpoint[],
  fe: Endpoint[],
  preserve: Endpoint[] = [],
): {
  endpoints: Endpoint[];
  frontend_only: Dangling[];
  backend_only: Dangling[];
} {
  const map = new Map<string, Endpoint>();
  for (const e of be) {
    const k = keyOf(e);
    const canonicalPath = normalizePathKey(e.path);
    map.set(k, {
      ...e,
      id: `${e.method} ${canonicalPath}`,
      path: canonicalPath,
      source: 'backend',
      source_files: {
        backend: [...(e.source_files.backend ?? [])],
        frontend: [],
      },
    });
  }
  for (const e of fe) {
    const k = keyOf(e);
    const existing = map.get(k);
    if (existing) {
      existing.source = 'both';
      existing.source_files.frontend = [
        ...(existing.source_files.frontend ?? []),
        ...(e.source_files.frontend ?? []),
      ];
      // Only preserve `status` when both sides agree; a disagreement
      // (one open, one closed) means one side regressed and the fix
      // card must be re-validated.
      if (existing.status !== e.status) {
        existing.status = undefined;
      }
    } else {
      const canonicalPath = normalizePathKey(e.path);
      map.set(k, {
        ...e,
        id: `${e.method} ${canonicalPath}`,
        path: canonicalPath,
        source: 'frontend',
      });
    }
  }
  for (const e of preserve) {
    const k = keyOf(e);
    if (map.has(k)) {
      // Scanner already knows this endpoint; copy any `status` marker
      // from the manually-curated entry into the scanner-derived one
      // so a previously-closed fix card stays closed.
      const existing = map.get(k)!;
      if (e.status && !existing.status) existing.status = e.status;
      continue;
    }
    const canonicalPath = normalizePathKey(e.path);
    map.set(k, {
      ...e,
      id: `${e.method} ${canonicalPath}`,
      path: canonicalPath,
      // Manually-curated entries are treated as `both` because the
      // fix-card author asserts the endpoint exists on both sides.
      source: 'both',
    });
  }

  const endpoints: Endpoint[] = [];
  const frontend_only: Dangling[] = [];
  const backend_only: Dangling[] = [];
  for (const e of map.values()) {
    endpoints.push(e);
    if (e.source === 'frontend') {
      frontend_only.push({
        kind: 'frontend-only',
        method: e.method,
        path: e.path,
        evidence: (e.source_files.frontend ?? []).map((f) => parseLoc(f)),
      });
    } else if (e.source === 'backend') {
      backend_only.push({
        kind: 'backend-only',
        method: e.method,
        path: e.path,
        evidence: (e.source_files.backend ?? []).map((f) => parseLoc(f)),
      });
    }
  }
  endpoints.sort((a, b) => (a.method + a.path).localeCompare(b.method + b.path));
  return { endpoints, frontend_only, backend_only };
}

function parseLoc(s: string): { file: string; line: number } {
  const [file, lineStr] = s.split(':');
  return { file: file ?? s, line: Number(lineStr ?? 0) };
}

export function unionEndpoints(
  be: Endpoint[],
  fe: Endpoint[],
  preserve: Endpoint[] = [],
): ContractFile {
  const { endpoints } = mergeByKey(be, fe, preserve);
  return {
    version: 1,
    generated_at: new Date().toISOString(),
    endpoints,
  };
}

export function checkContract(
  be: Endpoint[],
  fe: Endpoint[],
  preserve: Endpoint[] = [],
): CheckResult {
  const { endpoints, frontend_only, backend_only } = mergeByKey(be, fe, preserve);
  const paired = endpoints.filter((e) => e.source === 'both').length;
  return {
    ok: frontend_only.length === 0 && backend_only.length === 0,
    total_endpoints: endpoints.length,
    paired,
    frontend_only,
    backend_only,
  };
}
