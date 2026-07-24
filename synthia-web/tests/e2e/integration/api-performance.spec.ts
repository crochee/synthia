import { test, expect } from '@playwright/test';

/**
 * Layer 2 — API performance tests.
 * Validates that API endpoints respond within acceptable time limits.
 * Target: average response time < 500ms for management APIs (debug build).
 * Production builds with optimizations typically achieve < 100ms.
 */
test.describe('API performance', () => {
  const PERFORMANCE_THRESHOLD_MS = 500;

  test('health endpoint responds quickly', async ({ request }) => {
    const start = Date.now();
    const response = await request.get('/health');
    const duration = Date.now() - start;

    expect(response.ok()).toBe(true);
    expect(duration).toBeLessThan(PERFORMANCE_THRESHOLD_MS);
  });

  test('providers endpoint responds quickly', async ({ request }) => {
    const start = Date.now();
    const response = await request.get('/api/providers');
    const duration = Date.now() - start;

    expect(response.ok()).toBe(true);
    expect(duration).toBeLessThan(PERFORMANCE_THRESHOLD_MS);
  });

  test('skills endpoint responds quickly', async ({ request }) => {
    const start = Date.now();
    const response = await request.get('/api/skills');
    const duration = Date.now() - start;

    expect(response.ok()).toBe(true);
    expect(duration).toBeLessThan(PERFORMANCE_THRESHOLD_MS);
  });

  test('tools endpoint responds quickly', async ({ request }) => {
    const start = Date.now();
    const response = await request.get('/api/tools');
    const duration = Date.now() - start;

    expect(response.ok()).toBe(true);
    expect(duration).toBeLessThan(PERFORMANCE_THRESHOLD_MS);
  });

  test('settings endpoint responds quickly', async ({ request }) => {
    const start = Date.now();
    const response = await request.get('/api/settings');
    const duration = Date.now() - start;

    expect(response.ok()).toBe(true);
    expect(duration).toBeLessThan(PERFORMANCE_THRESHOLD_MS);
  });

  test('jobs endpoint responds quickly', async ({ request }) => {
    const start = Date.now();
    const response = await request.get('/api/jobs');
    const duration = Date.now() - start;

    expect(response.ok()).toBe(true);
    expect(duration).toBeLessThan(PERFORMANCE_THRESHOLD_MS);
  });

  test('tasks endpoint responds quickly', async ({ request }) => {
    const start = Date.now();
    const response = await request.get('/api/tasks');
    const duration = Date.now() - start;

    expect(response.ok()).toBe(true);
    expect(duration).toBeLessThan(PERFORMANCE_THRESHOLD_MS);
  });

  test('agent card endpoint responds quickly', async ({ request }) => {
    const start = Date.now();
    const response = await request.get('/.well-known/agent-card.json');
    const duration = Date.now() - start;

    expect(response.ok()).toBe(true);
    expect(duration).toBeLessThan(PERFORMANCE_THRESHOLD_MS);
  });
});
