/**
 * 共享类型：双侧契约
 * 设计参考: docs/interface-contract/SCHEMA.md (v1)
 */

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';

/**
 * JSON-RPC style method name used by the legacy chat surface.
 *
 * Retained as a string-literal union so historical contract.yaml
 * rows that still record `method: message:send` continue to parse.
 * The chat surface is a REST + SSE contract — there is no separate
 * RPC layer to enumerate beyond `message:send` itself.
 */
export type JsonRpcMethod = 'message:send';

export interface SseEventSpec {
  name: string;
  fields: string[];
  cadence_ms?: number;
  /**
   * Optional human note for the SSE event. Documents enum sets,
   * unit conventions, or special-case behaviour (e.g. "downgrade
   * unknown values to FAILED"). Not used by the scanner or check;
   * purely advisory.
   */
  notes?: string;
}

/**
 * Fix-card lifecycle marker for an endpoint.
 *
 * - `closed`: a fix card has been merged that aligns the two sides.
 *   The fixture / Playwright spec asserts the contract is now sound.
 * - `open` (default): no fix card has landed yet — the endpoint is a
 *   known inconsistency waiting for triage.
 *
 * Marked optional so older `contract.yaml` files without a `status`
 * key continue to parse; the scanner defaults to `undefined` (treated
 * as open in reports).
 */
export type EndpointStatus = 'open' | 'closed';

export interface Endpoint {
  id: string;
  method: HttpMethod | JsonRpcMethod;
  path: string;
  source: 'backend' | 'frontend' | 'both';
  source_files: {
    backend?: string[];
    frontend?: string[];
  };
  notes?: string;
  sse_events?: SseEventSpec[];
  status?: EndpointStatus;
}

export interface ContractFile {
  version: 1;
  generated_at: string;
  endpoints: Endpoint[];
}

export type DanglingKind = 'frontend-only' | 'backend-only';

export interface Dangling {
  kind: DanglingKind;
  method: string;
  path: string;
  evidence: { file: string; line: number }[];
}

export interface CheckResult {
  ok: boolean;
  total_endpoints: number;
  paired: number;
  frontend_only: Dangling[];
  backend_only: Dangling[];
}
