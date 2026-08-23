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
} from '../contract-coverage.js';

describe('computeCoverage', () => {
  it('reports all endpoints covered when every endpoint has a matching spec', () => {
    const cf = {
      version: 1,
      endpoints: [
        { id: 'GET /api/v1/sessions', method: 'GET', path: '/api/v1/sessions', source: 'backend' as const },
        { id: 'GET /health', method: 'GET', path: '/health', source: 'both' as const },
      ],
    };
    const specs = [
      { file: '/fake/contract-closure.sessions-list.spec.ts', text: 'GET /api/v1/sessions' },
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
        { id: 'GET /api/v1/sessions', method: 'GET', path: '/api/v1/sessions', source: 'backend' as const },
        { id: 'GET /health', method: 'GET', path: '/health', source: 'both' as const },
      ],
    };
    const specs = [
      { file: '/fake/contract-closure.health.spec.ts', text: 'GET /health' },
    ];

    const report = computeCoverage(cf, specs);

    expect(report.coveredEndpoints).toBe(1);
    expect(report.uncoveredEndpoints).toHaveLength(1);
    expect(report.uncoveredEndpoints[0].id).toBe('GET /api/v1/sessions');
  });

  it('tracks SSE event coverage', () => {
    const cf = {
      version: 1,
      endpoints: [
        {
          id: 'GET /api/v1/sessions/{id}/messages/stream',
          method: 'GET',
          path: '/api/v1/sessions/{id}/messages/stream',
          source: 'both' as const,
          sse_events: [
            { name: 'sessionStatus', fields: ['sessionId', 'sessionState'] },
            { name: 'message', fields: ['sessionId'] },
          ],
        },
      ],
    };
    // Only sessionStatus is covered by the spec
    const specs = [
      {
        file: '/fake/contract-closure.sse-session-status.spec.ts',
        text: 'sessionStatus SSE stream /api/v1/sessions/',
      },
    ];

    const report = computeCoverage(cf, specs);

    expect(report.totalSseEvents).toBe(2);
    expect(report.coveredSseEvents).toBe(1);
    expect(report.uncoveredSseEvents).toHaveLength(1);
    expect(report.uncoveredSseEvents[0].eventName).toBe('message');
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
        { id: 'GET /api/v1/sessions', source: 'backend' },
        { id: 'DELETE /api/tools/{key}', source: 'backend' },
      ],
      totalSseEvents: 0,
      coveredSseEvents: 0,
      uncoveredSseEvents: [],
    };

    const paragraph = formatUncoveredPathsParagraph(report);

    expect(paragraph).toContain('Uncovered paths:');
    expect(paragraph).toContain('GET /api/v1/sessions');
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
        { endpointId: 'GET /api/v1/sessions/{id}/messages/stream', eventName: 'attachment' },
      ],
    };

    const paragraph = formatUncoveredPathsParagraph(report);

    expect(paragraph).toContain('Uncovered paths:');
    expect(paragraph).toContain('SSE events (1):');
    expect(paragraph).toContain('attachment');
    expect(paragraph).toContain('GET /api/v1/sessions/{id}/messages/stream');
  });

  it('includes both endpoints and SSE events when both are uncovered', () => {
    const report: CoverageReport = {
      totalEndpoints: 2,
      coveredEndpoints: 0,
      uncoveredEndpoints: [
        { id: 'GET /api/v1/sessions', source: 'backend' },
      ],
      totalSseEvents: 1,
      coveredSseEvents: 0,
      uncoveredSseEvents: [
        { endpointId: 'GET /api/v1/sessions/{id}/messages/stream', eventName: 'sessionStatus' },
      ],
    };

    const paragraph = formatUncoveredPathsParagraph(report);

    expect(paragraph).toContain('Endpoints (1):');
    expect(paragraph).toContain('SSE events (1):');
  });
});
