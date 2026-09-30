/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_API_URL?: string;
  readonly VITE_CHAT_URL?: string;
  readonly VITE_WS_URL?: string;
  // Build identity baked by `Dockerfile.web` (mirrors the server's
  // `build_info`); see `synthia-web/src/lib/version.ts`.
  readonly VITE_SYNTHIA_VERSION?: string;
  readonly VITE_SYNTHIA_GIT_SHA?: string;
  readonly VITE_SYNTHIA_BUILD_TIME?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
