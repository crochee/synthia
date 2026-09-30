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
    expect(methods).toContain('GET /api/v1/sessions');
    expect(methods).toContain('GET /api/v1/sessions/');
    expect(methods).toContain('POST /api/v1/chat/sessions/messages');
    expect(methods).toContain('GET /api/tools');
  });

  it('records HTTP method when provided', () => {
    const eps = scanFrontendFile(FIX);
    const chat = eps.find(
      (e) => e.path === '/api/v1/chat/sessions/messages' && e.method === 'POST',
    )!;
    expect(chat).toBeDefined();
    expect(chat.method).toBe('POST');
  });

  it('records source_files.frontend', () => {
    const eps = scanFrontendFile(FIX);
    expect(eps.every((e) => e.source_files.frontend?.[0]).valueOf()).toBeTruthy();
  });
});
