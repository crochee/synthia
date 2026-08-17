/**
 * Bare-response HTTP client for the v1 Management API.
 *
 * The v1 API drops the legacy `{ status, data }` envelope: successful
 * responses return the resource directly as top-level JSON (or HTTP
 * 204 with no body for DELETE). Errors are signalled by HTTP status
 * code with a `{ code, message, result? }` JSON body.
 *
 * All Management API paths are versioned under `/api/v1/`. Callers
 * pass the full versioned path (e.g. `'/api/v1/skills'`); the
 * backend additionally emits 301 redirects from `/api/*` for
 * transitional clients, but explicit versioning keeps the wire
 * intent visible at the call site.
 */
const BASE_URL = import.meta.env.VITE_API_URL || '';
const API_KEY_STORAGE = 'synthia.apiKey';

/**
 * Base JSON header set. Hoisted to module scope so the request
 * path doesn't allocate a fresh `{ 'Content-Type': ... }` object
 * on every call. Auth is layered on top per-request (see below).
 */
const BASE_HEADERS: Readonly<Record<string, string>> = {
  'Content-Type': 'application/json',
};

/** Error body returned by the v1 API on non-2xx responses. */
interface ApiErrorBody {
  code?: string;
  message?: string;
  result?: unknown;
  /**
   * Some v1 handlers wrap the error detail under an `error`
   * sub-object (see `crates/synthia-server/src/error.rs::IntoResponse`
   * and the panic fallback in `middleware/error_handler.rs`).
   * Recognise both flat and nested shapes so the UI can show a
   * real message regardless of which handler produced the
   * response — otherwise we'd fall back to the generic
   * `HTTP <status>` text for half of the failure modes.
   */
  error?: { type?: string; code?: string; message?: string };
  status?: string;
}

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = BASE_URL) {
    this.baseUrl = baseUrl;
  }

  async get<T>(path: string, signal?: AbortSignal): Promise<T> {
    return this.request<T>('GET', path, undefined, signal);
  }

  async post<T>(path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
    return this.request<T>('POST', path, body, signal);
  }

  async put<T>(path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
    return this.request<T>('PUT', path, body, signal);
  }

  /**
   * DELETE — v1 returns HTTP 204 No Content with no body. Resolves
   * with `undefined` on success; throws on non-2xx.
   */
  async del(path: string, signal?: AbortSignal): Promise<void> {
    const response = await this.fetch('DELETE', path, undefined, signal);
    if (!response.ok) {
      throw await this.toError(response);
    }
    // Drain any accidental body so the connection can be reused,
    // then return undefined. Most 204 responses have no body.
    await response.text().catch(() => undefined);
  }

  private getApiKey(): string | null {
    try {
      return localStorage.getItem(API_KEY_STORAGE);
    } catch {
      return null;
    }
  }

  private async fetch(
    method: string,
    path: string,
    body?: unknown,
    signal?: AbortSignal,
  ): Promise<Response> {
    const url = `${this.baseUrl}${path}`;
    // Auth-gated merge: allocate a fresh header map only when an
    // API key is set. The unauthenticated hot path shares the
    // hoisted `BASE_HEADERS` reference directly.
    const apiKey = this.getApiKey();
    const headers = apiKey
      ? { ...BASE_HEADERS, Authorization: `Bearer ${apiKey}` }
      : BASE_HEADERS;
    const options: RequestInit = {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
      // Forward the caller's abort signal (if any) so list-page
      // navigations away from a half-loaded page can cancel the
      // in-flight fetch instead of letting it leak.
      signal,
    };
    return fetch(url, options);
  }

  /** Build an `Error` from a non-2xx `Response`, preferring the
   *  v1 error body. The server emits two different envelopes:
   *  - `ServerError::into_response` → `{"error": {"type", "message"}}`
   *    (see `crates/synthia-server/src/error.rs`).
   *  - panic fallback / auth middleware → `{"error": {"code", "message"}}`
   *    or `{"status": "error", "error": {"code", "message"}}`.
   *  We accept all three so the UI surfaces a real diagnostic
   *  message instead of the generic `HTTP <status>` placeholder. */
  private async toError(response: Response): Promise<Error> {
    let message = `HTTP ${response.status}`;
    try {
      const err = (await response.json()) as ApiErrorBody;
      // Nested `error.message` is the canonical v1 envelope.
      // Try it first because it's the most specific path.
      if (err.error?.message) {
        message = err.error.message;
      } else if (err.message) {
        message = err.message;
      } else if (err.error?.code || err.error?.type) {
        message = err.error.code ?? err.error.type ?? message;
      } else if (err.code) {
        message = err.code;
      }
    } catch {
      message = response.statusText || message;
    }
    return new Error(message);
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    signal?: AbortSignal,
  ): Promise<T> {
    const response = await this.fetch(method, path, body, signal);
    if (!response.ok) {
      throw await this.toError(response);
    }
    // 204 No Content — no JSON body to parse. Callers typing this
    // as `Promise<void>` get `undefined`; callers typing it as a
    // resource get `undefined` too, which is the expected v1 shape
    // for empty-body responses.
    if (response.status === 204) {
      return undefined as T;
    }
    // 304 Not Modified — caller must handle caching; we resolve
    // with `undefined` for type parity with 204.
    if (response.status === 304) {
      return undefined as T;
    }
    const text = await response.text();
    // Some 200/201 responses legitimately have an empty body (rare
    // in v1, but defensive). Treat empty as `undefined`.
    if (text.length === 0) return undefined as T;
    return JSON.parse(text) as T;
  }
}

export const api = new ApiClient();
