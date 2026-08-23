import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Backend bind address; overridable via `SYNTHIA_BACKEND_PORT`
// env var so dev-time runs against `:8081` etc. work without
// editing the file. Default `:8080` matches the
// `synthia-server` CLI default (`--port 8080`).
const BACKEND_PORT = process.env.SYNTHIA_BACKEND_PORT || '8080';
const BACKEND_TARGET = `http://localhost:${BACKEND_PORT}`;

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      // REST management API (tools/skills/providers/etc.)
      '/api': BACKEND_TARGET,
      // Readiness probe (server-health badge polls this)
      '/readyz': BACKEND_TARGET,
      // Liveness probe (kept for orchestrator-style checks)
      '/livez': BACKEND_TARGET,
    },
  },
  build: {
    // Split heavy vendor code into its own chunks so the
    // initial download stays small and so repeat visits can
    // skip already-cached vendor files.
    //
    // Why split manually rather than letting Rollup decide:
    // - Rollup's heuristic sometimes bundles `@radix-ui/themes`
    //   into the entry chunk, pushing it past 500 kB. The
    //   themes lib carries a lot of CSS-in-JS payloads we
    //   only need on the very first paint.
    // - React itself never changes between deploys, so a
    //   long-lived `react-vendor` chunk benefits from
    //   browser-disk caching across releases.
    rollupOptions: {
      output: {
        manualChunks: (id) => {
          if (id.includes('node_modules')) {
            if (id.includes('@radix-ui')) return 'vendor-radix';
            if (id.includes('react-router')) return 'vendor-router';
            if (id.includes('react-dom') || id.includes('/react/')) {
              return 'vendor-react';
            }
            // Anything else from node_modules (react-markdown
            // / remark / rehype / highlight.js) is already
            // behind the lazy <Markdown> chunk and flows
            // through Vite's automatic split. Don't claim
            // them here.
          }
          return undefined;
        },
      },
    },
  },
});
