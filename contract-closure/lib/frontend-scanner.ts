/**
 * 前端 fetch / @a2a-js/sdk 调用点扫描器
 * - 抽取 fetch(`<baseURL>${X}`, { method: "GET" | "POST" | ... })
 * - 占位符归一化：`${baseURL}` / template expressions → "/"
 * - 支持 const 绑定的间接 URL（HEALTH_URL = "/api/health"）解析。
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import type { Endpoint, HttpMethod } from './types.js';

/**
 * Pair-balanced fetch(...) matcher.
 * Captures from the opening `fetch(` through the matching `)`.
 * Handles nested parens (e.g. JSON.stringify(body)) and string templates.
 */
function findFetchCalls(src: string): { call: string; index: number }[] {
  const out: { call: string; index: number }[] = [];
  let i = 0;
  while (i < src.length) {
    const idx = src.indexOf('fetch(', i);
    if (idx === -1) break;
    let depth = 1;
    let j = idx + 6; // after "fetch("
    while (j < src.length && depth > 0) {
      const c = src[j];
      if (c === '(') depth++;
      else if (c === ')') depth--;
      if (depth === 0) break;
      j++;
    }
    if (depth === 0) {
      const call = src.slice(idx, j + 1);
      out.push({ call, index: idx });
      i = j + 1;
    } else {
      break;
    }
  }
  return out;
}

/**
 * Inside a single `fetch(...)` call (with parens balanced), split into the
 * URL argument and the options literal `{...}`. If no options literal exists,
 * `options` is null and the URL argument is the first comma-free token.
 */
function splitFetchArgs(call: string): { urlExpr: string; options: string | null } {
  // The call always begins with "fetch(".
  const inner = call.slice(6, -1); // drop "fetch(" and trailing ")"
  // Find the first top-level comma (not in template / not in nested paren).
  let depth = 0;
  let inStr: '"' | "'" | '`' | null = null;
  let escape = false;
  for (let i = 0; i < inner.length; i++) {
    const c = inner[i];
    if (escape) { escape = false; continue; }
    if (inStr) {
      if (c === '\\') { escape = true; continue; }
      if (c === inStr) inStr = null;
      continue;
    }
    if (c === '"' || c === "'" || c === '`') { inStr = c as '"' | "'" | '`'; continue; }
    if (c === '(' || c === '{' || c === '[') depth++;
    else if (c === ')' || c === '}' || c === ']') depth--;
    else if (c === ',' && depth === 0) {
      return { urlExpr: inner.slice(0, i), options: inner.slice(i + 1) };
    }
  }
  return { urlExpr: inner, options: null };
}

function findLineNumber(src: string, charIndex: number): number {
  let line = 1;
  for (let i = 0; i < charIndex && i < src.length; i++) {
    if (src[i] === '\n') line++;
  }
  return line;
}

function scanConstStringBindings(src: string): Map<string, string> {
  const out = new Map<string, string>();
  const re = /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*('([^']*)'|"([^"]*)"|`([^`]*)`)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const lit = m[3] ?? m[4] ?? m[5] ?? '';
    if (lit.startsWith('/')) out.set(m[1], lit);
  }
  return out;
}

/** Strip paired (same opening/closing) quote characters from a fragment.
 *  Backticks and other quote chars are also stripped individually even if
 *  unpaired (so `'foo\` becomes `foo`, treating each as plain punctuation). */
function stripQuotes(s: string): string {
  let out = s;
  while (out.length >= 1) {
    const first = out[0];
    const last = out[out.length - 1];
    if (first === '"' || first === "'" || first === '`') {
      out = out.slice(1);
    } else if (last === '"' || last === "'" || last === '`') {
      out = out.slice(0, -1);
    } else {
      break;
    }
  }
  return out;
}

function normalizePath(raw: string, consts: Map<string, string>): string | null {
  let s = raw.replace(/\s+/g, '');
  // Replace `${encodeURIComponent(<expr>)}` and the bare `encodeURIComponent(<expr>)`
  // with `{key}` — a single canonical placeholder name that matches the backend
  // router's `{key}` style. This way `encodeURIComponent(job.key)` → `{key}`
  // matches the corresponding backend `POST /api/jobs/{key}/pause`.
  s = s.replace(
    /\$\{[^}]*encodeURIComponent\([^)]*\)[^}]*\}|encodeURIComponent\([^)]*\)/g,
    '{key}',
  );
  // Substitute ${NAME} → const value (or empty string if unknown).
  s = s.replace(/\$\{([A-Za-z_$][\w$]*)\}/g, (_full, name) => {
    const v = consts.get(name);
    return v ?? '';
  });
  // Substitute bare NAME identifier → const value if known, else keep verbatim.
  s = s.replace(/[A-Za-z_$][\w$]*/g, (whole) => consts.get(whole) ?? whole);
  // Strip wrapper quotes / template backticks.
  s = stripQuotes(s);
  // Find the last "/something" path segment.
  // Path chars: /lettersdigits_-.{}:/
  const segRe = /\/([A-Za-z0-9_\-\.\{\}\/\:]+)/g;
  let lastMatch: string | null = null;
  let m: RegExpExecArray | null;
  while ((m = segRe.exec(s)) !== null) {
    lastMatch = '/' + m[1];
  }
  if (!lastMatch) return null;
  return lastMatch.replace(/\/+/g, '/');
}

export function scanFrontendFile(filePath: string): Endpoint[] {
  const src = readFileSync(filePath, 'utf8');
  const endpoints: Endpoint[] = [];
  const consts = scanConstStringBindings(src);

  const calls = findFetchCalls(src);
  for (const { call, index } of calls) {
    const { urlExpr, options } = splitFetchArgs(call);
    if (!urlExpr) continue;
    const path = normalizePath(urlExpr, consts);
    if (!path) continue;
    let method: HttpMethod = 'GET';
    if (options) {
      const methodMatch = /method\s*:\s*['"](GET|POST|PUT|DELETE|PATCH)['"]/i.exec(options);
      if (methodMatch) method = methodMatch[1].toUpperCase() as HttpMethod;
    }
    const line = findLineNumber(src, index);
    endpoints.push({
      id: `${method} ${path}`,
      method,
      path,
      source: 'frontend',
      source_files: { frontend: [`${filePath}:${line}`] },
    });
  }

  // Helper-style method calls: `api.get('...')`, `api.post<T>('...', body)`,
  // `httpClient.delete(...)`, `client.sendMessage(...)`. We do a regex-based
  // scan that captures `<callee>.<method>` and the first string argument,
  // mapping known aliases (get → GET, post → POST, etc., but also special
  // names like `sendMessage` → POST /a2a/message:send).
  const helperCalls = findHelperCalls(src, consts, filePath);
  for (const ep of helperCalls) {
    if (!endpoints.some((e) => e.id === ep.id)) endpoints.push(ep);
  }
  return endpoints;
}

interface HelperScanContext {
  source: string;
  consts: Map<string, string>;
}

function findHelperCalls(src: string, consts: Map<string, string>, filePath: string): Endpoint[] {
  const out: Endpoint[] = [];
  // Match identifiers followed by `.method<T?>(` followed by an open quote/template.
  // We deliberately restrict the helper set to ones we know how to map.
  const re = /\b([A-Za-z_$][\w$]*)\.(get|post|put|del|delete|patch|sendMessage|sendMessageStream|subscribeToTask)\s*(?:<[^>]*>)?\s*\(\s*([`'"`])/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const methodName = m[2];
    const quote = m[3];
    const argsStart = m.index + m[0].length;
    // Find the matching closing quote, handling backslash escapes and ${} interpolation.
    let i = argsStart;
    let depthBrace = 0;
    while (i < src.length) {
      const c = src[i];
      if (c === '\\') {
        i += 2;
        continue;
      }
      if (c === '$' && src[i + 1] === '{') {
        // Skip `${...}` interpolation. The expression can contain nested
        // function calls (e.g. `encodeURIComponent(id)`), so we must track
        // paren and brace depth independently while skipping the body.
        let jDepth = 0;
        i += 2;
        while (i < src.length) {
          const cc = src[i];
          if (cc === '{') jDepth++;
          else if (cc === '}') {
            if (jDepth === 0) {
              i++;
              break;
            }
            jDepth--;
          } else if (cc === '(') {
            let pDepth = 1;
            i++;
            while (i < src.length && pDepth > 0) {
              const pc = src[i];
              if (pc === '(') pDepth++;
              else if (pc === ')') pDepth--;
              else if (pc === '"' || pc === "'" || pc === '`') {
                const q = pc;
                i++;
                while (i < src.length && src[i] !== q) {
                  if (src[i] === '\\') i += 2;
                  else i++;
                }
              }
              i++;
            }
            continue;
          }
          i++;
        }
        continue;
      }
      if (c === quote) {
        break;
      }
      i++;
    }
    if (src[i] !== quote) continue;
    const expr = src.slice(argsStart, i);
    const path = normalizePath(expr, consts);
    if (!path) continue;

    // Map helper method to canonical HTTP verb.
    const map: Record<string, HttpMethod> = {
      get: 'GET',
      post: 'POST',
      put: 'PUT',
      del: 'DELETE',
      delete: 'DELETE',
      patch: 'PATCH',
      sendMessage: 'POST',
      sendMessageStream: 'POST',
      subscribeToTask: 'GET',
    };
    const method = map[methodName];
    if (!method) continue;

    // For sendMessage / sendMessageStream / subscribeToTask we replace the
    // path with the canonical A2A JSON-RPC path (which the user wrote
    // literally under /*/a2a/message:send or similar). The scanner can't
    // know those; we leave the extracted `path` as-is so the contract table
    // documents the raw path. The bridge to A2A JSON-RPC is documented in
    // ARBITRATION.md.
    const line = findLineNumber(src, m.index);
    out.push({
      id: `${method} ${path}`,
      method,
      path,
      source: 'frontend',
      source_files: { frontend: [`${filePath}:${line}`] },
    } as Endpoint);
  }
  return out;
}

export function scanFrontendDir(root: string): Endpoint[] {
  const out: Endpoint[] = [];
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop()!;
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      continue;
    }
    for (const name of entries) {
      const full = join(dir, name);
      let st;
      try { st = statSync(full); } catch { continue; }
      if (st.isDirectory()) {
        if (name === 'node_modules' || name === '__tests__' || name === 'dist') continue;
        stack.push(full);
      } else if (/\.(ts|tsx|js|jsx)$/.test(name)) {
        out.push(...scanFrontendFile(full));
      }
    }
  }
  return out;
}
