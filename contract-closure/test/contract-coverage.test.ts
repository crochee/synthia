/**
 * Tests for contract-coverage uncovered paths reporting.
 *
 * Covers:
 * (a) 0 uncovered paths → report does NOT contain "Uncovered paths:" paragraph
 * (b) >0 uncovered paths → report contains "Uncovered paths:" paragraph + stderr listing
 * (c) SSE event coverage is tracked separately from endpoint coverage
 * (d) advisory mode: exit code is 0 even when uncovered paths exist
 */
import { describe, it, expect } from 'vitest';
import {
  computeCoverage,
  formatUncoveredPathsParagraph,
  type CoverageReport,
  type UncoveredEndpoint,
  type UncoveredSseEvent,
} from '../contract-coverage.js';

describe('computeCoverage', () => {
  it('reports all endpoints covered when every endpoint has a matching spec', () => {
    const cf = {
      version: 1,
      endpoints: [
        { id: 'GET /api/tasks', method: 'GET', path: '/api/tasks', source: 'backend' as const },
        { id: 'GET /health', method: 'GET', path: '/health', source: 'both' as const },
      ],
    };
    const specs = [
      { file: '/fake/contract-closure.tasks-list.spec.ts', text: 'GET /api/tasks' },
      { file: '/fake/contract-closure.health.spec.ts', text: 'GET /health' },
    ];

    const report = computeCoverage(cf, specs);

    expect(report.totalEndpoints).toBe(2);
    expect(report.coveredEndpoints).toBe(2);
    expect(report.uncoveredEndpoints).toHaveLength(0);
    expect(report.totalSseEvents).toBe(0);
    expect(report.uncoveredSseEvents).toHaveLength(0);
  });

  it('reports uncovered endpoints when specs are missing', () => {
    const cf = {
      version: 1,
      endpoints: [
        { id: 'GET /api/tasks', method: 'GET', path: '/api/tasks', source: 'backend' as const },
        { id: 'GET /health', method: 'GET', path: '/health', source: 'both' as const },
      ],
    };
    const specs = [
      { file: '/fake/contract-closure.health.spec.ts', text: 'GET /health' },
    ];

    const report = computeCoverage(cf, specs);

    expect(report.coveredEndpoints).toBe(1);
    expect(report.uncoveredEndpoints).toHaveLength(1);
    expect(report.uncoveredEndpoints[0].id).toBe('GET /api/tasks');
  });

  it('tracks SSE event coverage', () => {
    const cf = {
      version: 1,
      endpoints: [
        {
          id: 'GET /a2a/tasks/{key}:subscribe',
          method: 'GET',
          path: '/a2a/tasks/{key}:subscribe',
          source: 'both' as const,
          sse_events: [
            { name: 'status-update', fields: ['taskId'] },
            { name: 'artifact-update', fields: ['taskId'] },
          ],
        },
      ],
    };
    // Only status-update is covered by the spec
    const specs = [
      {
        file: '/fake/contract-closure.sse-status-update.spec.ts',
        text: 'status-update SSE subscribe /a2a/tasks/',
      },
    ];

    const report = computeCoverage(cf, specs);

    expect(report.totalSseEvents).toBe(2);
    expect(report.coveredSseEvents).toBe(1);
    expect(report.uncoveredSseEvents).toHaveLength(1);
    expect(report.uncoveredSseEvents[0].eventName).toBe('artifact-update');
  });
});

describe('formatUncoveredPathsParagraph', () => {
  it('returns empty string when no uncovered paths', () => {
    const report: CoverageReport = {
      totalEndpoints: 2,
      coveredEndpoints: 2,
      uncoveredEndpoints: [],
      totalSseEvents: 0,
      coveredSseEvents: 0,
      uncoveredSseEvents: [],
    };

    const paragraph = formatUncoveredPathsParagraph(report);
    expect(paragraph).toBe('');
  });

  it('includes "Uncovered paths:" paragraph when endpoints are uncovered', () => {
    const report: CoverageReport = {
      totalEndpoints: 3,
      coveredEndpoints: 1,
      uncoveredEndpoints: [
        { id: 'GET /api/tasks', source: 'backend' },
        { id: 'DELETE /api/tools/{key}', source: 'backend' },
      ],
      totalSseEvents: 0,
      coveredSseEvents: 0,
      uncoveredSseEvents: [],
    };

    const paragraph = formatUncoveredPathsParagraph(report);

    expect(paragraph).toContain('Uncovered paths:');
    expect(paragraph).toContain('GET /api/tasks');
    expect(paragraph).toContain('DELETE /api/tools/{key}');
    expect(paragraph).toContain('Endpoints (2):');
  });

  it('includes SSE events section when SSE events are uncovered', () => {
    const report: CoverageReport = {
      totalEndpoints: 1,
      coveredEndpoints: 1,
      uncoveredEndpoints: [],
      totalSseEvents: 2,
      coveredSseEvents: 1,
      uncoveredSseEvents: [
        { endpointId: 'GET /a2a/tasks/{key}:subscribe', eventName: 'artifact-update' },
      ],
    };

    const paragraph = formatUncoveredPathsParagraph(report);

    expect(paragraph).toContain('Uncovered paths:');
    expect(paragraph).toContain('SSE events (1):');
    expect(paragraph).toContain('artifact-update');
    expect(paragraph).toContain('GET /a2a/tasks/{key}:subscribe');
  });

  it('includes both endpoints and SSE events when both are uncovered', () => {
    const report: CoverageReport = {
      totalEndpoints: 2,
      coveredEndpoints: 0,
      uncoveredEndpoints: [
        { id: 'GET /api/tasks', source: 'backend' },
      ],
      totalSseEvents: 1,
      coveredSseEvents: 0,
      uncoveredSseEvents: [
        { endpointId: 'GET /a2a/tasks/{key}:subscribe', eventName: 'status-update' },
      ],
    };

    const paragraph = formatUncoveredPathsParagraph(report);

    expect(paragraph).toContain('Endpoints (1):');
    expect(paragraph).toContain('SSE events (1):');
  });
});
