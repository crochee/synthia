/**
 * 共享类型：双侧契约
 * 设计参考: docs/interface-contract/SCHEMA.md (v1)
 */

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';

export type A2AJsonRpcMethod = 'message:send' | 'tasks:get' | 'tasks:cancel';

export interface SseEventSpec {
  name: string;
  fields: string[];
  cadence_ms?: number;
}

export interface Endpoint {
  id: string;
  method: HttpMethod | A2AJsonRpcMethod;
  path: string;
  source: 'backend' | 'frontend' | 'both';
  source_files: {
    backend?: string[];
    frontend?: string[];
  };
  notes?: string;
  sse_events?: SseEventSpec[];
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
