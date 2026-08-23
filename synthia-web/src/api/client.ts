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
// `import.meta.env` is a Vite-only injection — when the
// module is loaded by Node (e.g. Playwright's collection
// phase which pulls in `src/lib/session-to-messages.ts` for the
// unit tests, transitively reaching this client), `import.meta`
// is `undefined` and accessing `.env` would throw. Guard the
// lookup so the module is safe to load under either runtime;
// the browser still gets the Vite-injected base URL.
const BASE_URL = (() => {
  try {
    return import.meta.env.VITE_API_URL ?? '';
  } catch {
    return '';
  }
})();
const API_KEY_STORAGE = 'synthia.apiKey';

/**
 * Base JSON header set. Hoisted to module scope so the request
 * path doesn't allocate a fresh `{ 'Content-Type': ... }` object
 * on every call. Auth is layered on top per-request (see below).
 */
const BASE_HEADERS: Readonly<Record<string, string>> = {
  'Content-Type': 'application/json',
};

/** Wire shape of every non-2xx response from the v1 API.
 *
 * The server emits one flat envelope on every error path:
 * `{"code", "message"}` via `synthia_server::api::AppError`
 * (see `crates/synthia-server/src/api/error.rs`) and the panic
 * fallback in `middleware/error_handler.rs` reuses the same
 * adapter. */
interface ApiErrorBody {
  code: string;
  message: string;
}

/** Structured error thrown by [`ApiClient`] on non-2xx responses.
 *
 * Extends the native `Error` so existing call sites that read
 * `err.message` keep working unchanged; new code can branch on
 * `err.code` (e.g. `"not_found"`, `"validation"`) or `err.status`
 * (HTTP status code) for structured UI handling. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }
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

  async patch<T>(path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
    return this.request<T>('PATCH', path, body, signal);
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
    const headers = apiKey ? { ...BASE_HEADERS, Authorization: `Bearer ${apiKey}` } : BASE_HEADERS;
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

  /** Build an [`ApiError`] from a non-2xx `Response`. Parses the
   *  flat envelope `{"code", "message"}` produced by
   *  `synthia_server::api::AppError` and the panic fallback in
   *  `middleware/error_handler.rs` (both route through the same
   *  adapter). Falls back to the raw response status / status
   *  text when the body is missing or unparseable (e.g. an HTML
   *  502 from an upstream proxy). */
  private async toError(response: Response): Promise<ApiError> {
    const { status } = response;
    let code = `http_${status}`;
    let message = response.statusText || `HTTP ${status}`;
    try {
      const body = (await response.json()) as ApiErrorBody;
      if (body.code) code = body.code;
      if (body.message) message = body.message;
    } catch {
      // Non-JSON body (e.g. reverse-proxy 502) — keep the status
      // placeholder message.
    }
    return new ApiError(status, code, message);
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
