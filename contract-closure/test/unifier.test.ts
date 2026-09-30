import { describe, expect, it } from 'vitest';
import { checkContract, unionEndpoints } from '../lib/unifier.js';
import type { Endpoint } from '../lib/types.js';

const be: Endpoint[] = [
  { id: 'GET /api/health',         method: 'GET',    path: '/api/health',         source: 'backend', source_files: { backend: ['router.rs:1'] } },
  { id: 'GET /api/tasks',          method: 'GET',    path: '/api/tasks',          source: 'backend', source_files: { backend: ['router.rs:2'] } },
  { id: 'POST /api/tasks',         method: 'POST',   path: '/api/tasks',          source: 'backend', source_files: { backend: ['router.rs:3'] } },
  { id: 'GET /api/tools',          method: 'GET',    path: '/api/tools',          source: 'backend', source_files: { backend: ['router.rs:4'] } },
  { id: 'DELETE /api/jobs/{key}',  method: 'DELETE', path: '/api/jobs/{key}',     source: 'backend', source_files: { backend: ['router.rs:5'] } },
  { id: 'DELETE /api/mcp/servers/{id}', method: 'DELETE', path: '/api/mcp/servers/{id}', source: 'backend', source_files: { backend: ['router.rs:6'] } },
];

const fe: Endpoint[] = [
  { id: 'GET /api/health',         method: 'GET',    path: '/api/health',         source: 'frontend', source_files: { frontend: ['health.ts:5'] } },
  { id: 'GET /api/tasks',          method: 'GET',    path: '/api/tasks',          source: 'frontend', source_files: { frontend: ['tasks.ts:10'] } },
  { id: 'POST /api/tasks',         method: 'POST',   path: '/api/tasks',          source: 'frontend', source_files: { frontend: ['tasks.ts:15'] } },
  { id: 'GET /synthia/health',     method: 'GET',    path: '/synthia/health',     source: 'frontend', source_files: { frontend: ['rogue.ts:1'] } },
  // Frontend uses canonicalised `{key}` placeholder; backend uses `{id}`. They should pair.
  { id: 'DELETE /api/mcp/servers/{key}', method: 'DELETE', path: '/api/mcp/servers/{key}', source: 'frontend', source_files: { frontend: ['mcp.ts:63'] } },
];

describe('unionEndpoints', () => {
  it('marks both-sides as source=both and merges source_files', () => {
    const cf = unionEndpoints(be, fe);
    const health = cf.endpoints.find((e) => e.id === 'GET /api/health')!;
    expect(health.source).toBe('both');
    expect(health.source_files.backend).toContain('router.rs:1');
    expect(health.source_files.frontend).toContain('health.ts:5');
  });

  it('keeps backend-only entries with source=backend', () => {
    const cf = unionEndpoints(be, fe);
    const tools = cf.endpoints.find((e) => e.id === 'GET /api/tools')!;
    expect(tools.source).toBe('backend');
  });

  it('keeps frontend-only entries', () => {
    const cf = unionEndpoints(be, fe);
    const rogue = cf.endpoints.find((e) => e.id === 'GET /synthia/health');
    expect(rogue).toBeDefined();
    expect(rogue?.source).toBe('frontend');
  });

  it('pairs entries with different placeholder names (e.g. {id} vs {key})', () => {
    const cf = unionEndpoints(be, fe);
    const ep = cf.endpoints.find((e) => e.id === 'DELETE /api/mcp/servers/{key}')!;
    expect(ep).toBeDefined();
    expect(ep.source).toBe('both');
    // Path key inside the contract is the canonical `{key}` form (frontend input).
    expect(ep.path).toBe('/api/mcp/servers/{key}');
  });
});

describe('checkContract', () => {
  it('returns ok=false when there are frontend-only endpoints', () => {
    const res = checkContract(be, fe);
    expect(res.ok).toBe(false);
    expect(res.frontend_only.length).toBeGreaterThan(0);
    expect(res.frontend_only[0].kind).toBe('frontend-only');
  });

  it('returns frontend_only=0 when frontend-side mirrors a subset of backend', () => {
    // NOTE: `checkContract` exposes `ok: false` whenever either side has dangling
    // entries. CI currently uses `frontend_only > 0` as the blocking signal;
    // backend-only endpoints are accepted as advisory (they may simply be
    // features the UI hasn't wired up yet). This test mirrors that policy.
    const subset = be
      .filter(
        (e) =>
          e.id !== 'DELETE /api/jobs/{key}' &&
          e.id !== 'GET /api/tools' &&
          e.id !== 'DELETE /api/mcp/servers/{id}',
      )
      .map((b) => {
        const normalisedPath = b.path.replace(/\{[A-Za-z_$][\w$]*\}/g, '{key}');
        return {
          ...b,
          id: `${b.method} ${normalisedPath}`,
          path: normalisedPath,
          source: 'frontend' as const,
          source_files: { frontend: ['mock-fe.ts:1'] },
        };
      });
    const res = checkContract(be, subset);
    expect(res.frontend_only.length).toBe(0);
    // subset excludes 3 backend-only endpoints, so backend_only reports those.
    expect(res.backend_only.length).toBeGreaterThan(0);
    expect(res.paired).toBe(subset.length);
  });

  it('returns ok=true only when both sides completely mirror each other', () => {
    // Restrict be to a strict subset that subset will fully cover.
    const smallBe = be.filter(
      (e) =>
        e.id !== 'DELETE /api/jobs/{key}' &&
        e.id !== 'GET /api/tools' &&
        e.id !== 'DELETE /api/mcp/servers/{id}',
    );
    const mirror = smallBe.map((b) => {
      const normalisedPath = b.path.replace(/\{[A-Za-z_$][\w$]*\}/g, '{key}');
      return {
        ...b,
        id: `${b.method} ${normalisedPath}`,
        path: normalisedPath,
        source: 'frontend' as const,
        source_files: { frontend: ['mock-fe.ts:1'] },
      };
    });
    const res = checkContract(smallBe, mirror);
    expect(res.ok).toBe(true);
    expect(res.frontend_only.length).toBe(0);
    expect(res.backend_only.length).toBe(0);
  });
});
