/**
 * 后端 axum router 扫描器
 *
 * - 抽取 ".route(path, get|post|delete|put|patch(handler))" 与
 *   ".route(path, get(...).post(...))" 形式注册的路由
 * - 解析 `let VAR = Router::new()…route(…).route(…);` blocks — 块的范围
 *   从 `let VAR = Router::new` 起到下一个 top-level `;` / `}` 为止（即使
 *   Router::new() 紧跟 `)` 后是链式调用）。每个 `.route` 归属于一个 var。
 * - 解析 `.nest("/prefix", VAR)` 与 `.nest_service("/prefix", VAR)`，
 *   把变量内 .route 的 path 加上 prefix
 */
import { readFileSync } from 'node:fs';
import type { Endpoint, HttpMethod } from './types.js';

// `.route("...", <handler chain>)`. The path regex allows newlines so that
// multi-line route calls like
//   .route(
//       "/.well-known/agent-card.json",
//       get(...),
//   )
// are still matched.
const ROUTE_RE = /\.route\(\s*"([^"]+)"\s*,/g;
const METHOD_TOKEN_RE = /\b(get|post|put|delete|patch)\s*\(/gi;
const VAR_LET_RE = /\blet\s+([A-Za-z_$][\w$]*)\s*=\s*Router::new\b/g;
const NEST_RE = /\.nest(?:_service)?\s*\(\s*"([^"]+)"\s*,\s*([A-Za-z_$][\w$]*)/g;

function methodsInHandlerChain(chain: string): HttpMethod[] {
  const out: HttpMethod[] = [];
  METHOD_TOKEN_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = METHOD_TOKEN_RE.exec(chain)) !== null) {
    const tok = m[1].toUpperCase() as HttpMethod;
    if (!out.includes(tok)) out.push(tok);
  }
  return out;
}

interface Block {
  varName: string | null;
  start: number;
  end: number;
}

/**
 * Find blocks of the form:
 *   let VAR = Router::new(...);
 *   let VAR = Router::new(...).route(...).route(...);
 *   let VAR = Router::new()
 *       .route(...);
 *
 * A block's end is the top-level `;` that closes the chain, or a matching
 * `}` that closes a containing block. We track paren + string-literal +
 * line-comment balance to skip over noisy content.
 */
function findRouterBlocks(src: string): Block[] {
  const blocks: Block[] = [];
  let cursor = 0;
  while (cursor < src.length) {
    VAR_LET_RE.lastIndex = cursor;
    const m = VAR_LET_RE.exec(src);
    if (!m) break;
    const varName = m[1];
    const after = VAR_LET_RE.lastIndex; // position right after `Router::new`
    // Skip whitespace.
    let i = after;
    while (i < src.length && /\s/.test(src[i])) i++;
    // If the next char is `(`, walk paren-balanced.
    let startChain = i;
    if (src[i] === '(') {
      let depth = 1;
      i++;
      while (i < src.length && depth > 0) {
        const c = src[i];
        if (c === '(') depth++;
        else if (c === ')') depth--;
        else if (c === '"') {
          i++;
          while (i < src.length && src[i] !== '"') {
            if (src[i] === '\\') i += 2;
            else i++;
          }
        } else if (c === '/' && src[i + 1] === '/') {
          while (i < src.length && src[i] !== '\n') i++;
        }
        i++;
      }
      startChain = i;
    }
    // Now walk the chained `.foo(...)` segment until next `;` or `}` at depth 0.
    let chainDepth = 0;
    while (i < src.length) {
      const c = src[i];
      if (c === '(' || c === '[' || c === '{') chainDepth++;
      else if (c === ')' || c === ']' || c === '}') {
        if (chainDepth === 0) break;
        chainDepth--;
      } else if (c === ';') {
        if (chainDepth === 0) {
          i++;
          break;
        }
      } else if (c === '"') {
        i++;
        while (i < src.length && src[i] !== '"') {
          if (src[i] === '\\') i += 2;
          else i++;
        }
      } else if (c === '/' && src[i + 1] === '/') {
        while (i < src.length && src[i] !== '\n') i++;
        continue;
      }
      i++;
    }
    blocks.push({ varName, start: m.index, end: i });
    cursor = i;
  }
  return blocks;
}

function findNests(src: string): Map<string, string[]> {
  const out = new Map<string, string[]>();
  NEST_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = NEST_RE.exec(src)) !== null) {
    const prefix = m[1];
    const varName = m[2];
    const arr = out.get(varName) ?? [];
    arr.push(prefix);
    out.set(varName, arr);
  }
  return out;
}

function lineOfSrc(src: string, offset: number): number {
  let line = 1;
  for (let i = 0; i < offset && i < src.length; i++) {
    if (src[i] === '\n') line++;
  }
  return line;
}

function joinPath(prefix: string, path: string): string {
  if (prefix === '/' && path === '/') return '/';
  if (path === '/') return prefix || '/';
  if (prefix.endsWith('/') && path.startsWith('/')) return prefix + path.slice(1);
  if (!prefix.endsWith('/') && !path.startsWith('/')) return (prefix || '') + '/' + path;
  return prefix + path;
}

/**
 * Walk forward from a `.route("path", <X>)` opening paren until the matching
 * close, parsing out HTTP method tokens (get/post/put/delete/patch) used
 * inside the route handler chain (either as a direct method receiver like
 * `get(handler)` or as part of a method-router chain like
 * `get(h1).post(h2)`).
 */
function parseHandlerChain(src: string, openParenIdx: number): { methods: HttpMethod[]; closeIdx: number } {
  // openParenIdx is the position of the `(` after .route("path"
  let depth = 1;
  let i = openParenIdx + 1;
  while (i < src.length && depth > 0) {
    const c = src[i];
    if (c === '(') depth++;
    else if (c === ')') depth--;
    else if (c === '"') {
      i++;
      while (i < src.length && src[i] !== '"') {
        if (src[i] === '\\') i += 2;
        else i++;
      }
    } else if (c === '/' && src[i + 1] === '/') {
      while (i < src.length && src[i] !== '\n') i++;
    }
    i++;
  }
  const closeIdx = i - 1;
  const chain = src.slice(openParenIdx + 1, closeIdx);
  return { methods: methodsInHandlerChain(chain), closeIdx };
}

export function scanBackendRouter(filePath: string): Endpoint[] {
  const src = readFileSync(filePath, 'utf8');
  const blocks = findRouterBlocks(src);
  const nests = findNests(src);

  // First pass: find every `.route("path", ` and resolve the path, owning block,
  // and HTTP method tokens within the handler chain.
  interface Hit {
    path: string;
    methods: HttpMethod[];
    offset: number; // offset of the `.route(`
    varName: string | null;
    prefixes: string[];
  }
  const hits: Hit[] = [];
  const OPEN_PAREN_RE = /\.route\(\s*"([^"]+)"\s*,/g;
  let m: RegExpExecArray | null;
  while ((m = OPEN_PAREN_RE.exec(src)) !== null) {
    const path = m[1];
    const openParenOffset = m.index + m[0].length - 1; // index of '('
    const { methods } = parseHandlerChain(src, openParenOffset);
    // Find the owning block — the most recent block whose end > m.index.
    let owning: Block | null = null;
    for (const b of blocks) {
      if (b.start <= m.index && b.end > m.index) owning = b;
    }
    const prefixes = owning && owning.varName ? nests.get(owning.varName) ?? [] : [];
    hits.push({
      path,
      methods,
      offset: m.index,
      varName: owning ? owning.varName : null,
      prefixes,
    });
  }

  const endpoints: Endpoint[] = [];
  for (const h of hits) {
    const mountedPaths =
      h.prefixes.length === 0 ? [h.path] : h.prefixes.map((p) => joinPath(p, h.path));
    const line = lineOfSrc(src, h.offset);
    for (const mounted of mountedPaths) {
      for (const method of h.methods) {
        endpoints.push({
          id: `${method} ${mounted}`,
          method,
          path: mounted,
          source: 'backend',
          source_files: { backend: [`${filePath}:${line}`] },
        });
      }
    }
  }
  return endpoints;
}
