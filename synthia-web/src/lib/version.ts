// Build identity surfaced to the React UI.
//
// Vite bakes `import.meta.env.VITE_*` at build time; the values come
// from `Dockerfile.web`'s `ARG` / `ENV` block (or, in dev, from a
// `.env.local`). They mirror the server's `build_info::VERSION` /
// `GIT_SHA` / `BUILD_TIME` so an operator looking at the running
// browser tab sees the same identity as `GET /version` on the API.
//
// All three values are optional: a build without the env vars
// yields empty strings, which `displayVersion()` collapses to a
// single `unknown` token rather than a noisy `v · ` placeholder.
// `displayShortSha()` shortens the SHA the same way the server's
// `build_info::short_sha()` does (`<sha>[..7]`), so server and UI
// always agree on what to show.

export interface BuildIdentity {
  version: string;
  gitSha: string;
  shortSha: string;
  buildTime: string;
}

const VERSION: string = import.meta.env.VITE_SYNTHIA_VERSION ?? '';
const GIT_SHA: string = import.meta.env.VITE_SYNTHIA_GIT_SHA ?? '';
const BUILD_TIME: string = import.meta.env.VITE_SYNTHIA_BUILD_TIME ?? '';

export const buildIdentity: BuildIdentity = {
  version: VERSION,
  gitSha: GIT_SHA,
  shortSha: GIT_SHA.length >= 7 ? GIT_SHA.slice(0, 7) : GIT_SHA,
  buildTime: BUILD_TIME,
};

/// `v<version> · <shortSha>` — the same shape the server's
/// `--version` line carries. Empty fields collapse to `unknown`
/// so a developer-mode build doesn't show `v ·  `.
export function displayVersion(): string {
  const v = VERSION || 'unknown';
  const sha = buildIdentity.shortSha || 'unknown';
  return `v${v} · ${sha}`;
}
