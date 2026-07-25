import { describe, expect, it } from 'vitest';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { scanFrontendFile } from '../lib/frontend-scanner.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIX = join(__dirname, '..', '__fixtures__', 'frontend', 'sample-fetch.ts');

describe('scanFrontendFile', () => {
  it('extracts GET and POST fetch calls', () => {
    const eps = scanFrontendFile(FIX);
    const methods = eps.map((e) => `${e.method} ${e.path}`).sort();
    expect(methods).toContain('GET /api/health');
    expect(methods).toContain('GET /api/tasks');
    expect(methods).toContain('POST /api/tasks');
    expect(methods).toContain('GET /api/tools');
    expect(methods).toContain('POST /a2a/message:send');
  });

  it('records HTTP method when provided', () => {
    const eps = scanFrontendFile(FIX);
    const tasks = eps.find((e) => e.path === '/api/tasks' && e.method === 'POST')!;
    expect(tasks).toBeDefined();
    expect(tasks.method).toBe('POST');
  });

  it('records source_files.frontend', () => {
    const eps = scanFrontendFile(FIX);
    expect(eps.every((e) => e.source_files.frontend?.[0]).valueOf()).toBeTruthy();
  });
});
