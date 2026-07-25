import { describe, expect, it } from 'vitest';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { scanBackendRouter } from '../lib/backend-scanner.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(__dirname, '..', '__fixtures__', 'backend', 'sample-router.rs');

describe('scanBackendRouter', () => {
  it('extracts single-method .route calls without prefix', () => {
    const eps = scanBackendRouter(FIXTURE);
    expect(eps.find((e) => e.id === 'GET /health')).toBeDefined();
    expect(
      eps.find((e) => e.id === 'GET /.well-known/agent-card.json'),
    ).toBeDefined();
  });

  it('handles same path with multiple methods within a single Router::new() chain', () => {
    const eps = scanBackendRouter(FIXTURE);
    expect(eps.find((e) => e.id === 'GET /api/tasks')).toBeDefined();
    expect(eps.find((e) => e.id === 'POST /api/tasks')).toBeDefined();
  });

  it('applies nest("/api", api_routes) prefix to api_routes routes', () => {
    const eps = scanBackendRouter(FIXTURE);
    expect(eps.find((e) => e.id === 'GET /api/models')).toBeDefined();
    expect(eps.find((e) => e.id === 'DELETE /api/jobs/{key}')).toBeDefined();
    expect(eps.find((e) => e.id === 'POST /api/jobs/{key}/execute')).toBeDefined();
  });

  it('applies nested "/api/approvals" prefix with attached root "/" → /api/approvals', () => {
    const eps = scanBackendRouter(FIXTURE);
    expect(eps.find((e) => e.id === 'GET /api/approvals')).toBeDefined();
    expect(eps.find((e) => e.id === 'POST /api/approvals/{id}/resolve')).toBeDefined();
  });

  it('handles one-line chained Router::new().route(...) (ws_routes pattern)', () => {
    const eps = scanBackendRouter(FIXTURE);
    // ws_routes is merged directly (not nested) → no prefix.
    expect(eps.find((e) => e.id === 'GET /ws/approvals')).toBeDefined();
  });

  it('captures source file pointer', () => {
    const eps = scanBackendRouter(FIXTURE);
    const e = eps.find((e) => e.id === 'GET /health');
    expect(e).toBeDefined();
    expect(e!.source_files.backend?.[0]).toContain('sample-router.rs');
  });
});
