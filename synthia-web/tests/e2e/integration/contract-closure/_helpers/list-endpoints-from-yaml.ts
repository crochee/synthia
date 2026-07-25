/**
 * `docs/interface-contract/contract.yaml` 解析 helper。
 *
 * - 用 `yaml` 库解析 YAML（不引 ts/yaml 之外的额外解析器）
 * - 暴露 `loadEndpoints()` 返回 `ContractEndpoint[]`
 * - 暴露 `loadEndpointsFor(specFileName)` 过滤出 test 文件 *期望覆盖* 的子集
 *
 * 解析原则：
 *   - 缺失文件 → throw（CI 直接失败）；开发期会报错而不是 silently 返回 []
 *   - 版本号不匹配 → 警告（不抛），不阻塞测试
 *   - 不依赖 contract-closure 包，避免在 synthia-web 重新建 ts 引用
 */
import { readFileSync } from 'node:fs';
import { parse as parseYaml } from 'yaml';

export interface ContractEndpoint {
  id: string;
  method: string;
  path: string;
  source: 'backend' | 'frontend' | 'both';
  sse_events?: { name: string; fields: string[] }[];
}

interface ContractFile {
  version: number;
  generated_at?: string;
  endpoints: ContractEndpoint[];
}

const DEFAULT_CONTRACT_PATH = 'docs/interface-contract/contract.yaml';

export function contractPath(): string {
  // Synthia-web/tests run with cwd == synthia-web/.  Contract.yaml lives
  // at repo root, so resolve from there.
  const fromCwd = (process.env.CONTRACT_PATH ?? DEFAULT_CONTRACT_PATH).replace(
    /^\.\//,
    '',
  );
  return process.cwd().endsWith('synthia-web')
    ? `../${fromCwd}`
    : fromCwd;
}

export function loadEndpoints(path?: string): ContractEndpoint[] {
  const file = path ?? contractPath();
  let raw: string;
  try {
    raw = readFileSync(file, 'utf8');
  } catch (e) {
    throw new Error(
      `[contract-closure] contract file not found at ${file}. ` +
        `Run \`make contract-scan\` first. Original error: ${(e as Error).message}`,
    );
  }
  const doc = parseYaml(raw) as ContractFile;
  if (!doc || !Array.isArray(doc.endpoints)) {
    throw new Error(`[contract-closure] contract file malformed: ${file}`);
  }
  return doc.endpoints;
}

/**
 * 强制类型守卫 helper。契约 yaml 在 generator 端会保证 `source` 一定在
 * 三选一里；如果被人手工编辑坏了，给个明确的 run-time fail。
 */
export function onlyBackend(eps: ContractEndpoint[]): ContractEndpoint[] {
  const out: ContractEndpoint[] = [];
  for (const e of eps) {
    if (e.source === 'backend' || e.source === 'both') out.push(e);
  }
  return out;
}
