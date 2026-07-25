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

function mergeByKey(be: Endpoint[], fe: Endpoint[]): {
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

export function unionEndpoints(be: Endpoint[], fe: Endpoint[]): ContractFile {
  const { endpoints } = mergeByKey(be, fe);
  return {
    version: 1,
    generated_at: new Date().toISOString(),
    endpoints,
  };
}

export function checkContract(be: Endpoint[], fe: Endpoint[]): CheckResult {
  const { endpoints, frontend_only, backend_only } = mergeByKey(be, fe);
  const paired = endpoints.filter((e) => e.source === 'both').length;
  return {
    ok: frontend_only.length === 0 && backend_only.length === 0,
    total_endpoints: endpoints.length,
    paired,
    frontend_only,
    backend_only,
  };
}
