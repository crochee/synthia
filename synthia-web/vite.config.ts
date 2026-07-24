import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      // REST management API (tools/skills/providers/etc.)
      '/api': 'http://localhost:8080',
      // A2A protocol endpoints (JSON-RPC + SSE)
      '/a2a': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        // SSE streaming needs buffering disabled
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq) => {
            proxyReq.setHeader('Connection', 'keep-alive');
          });
        },
      },
      // Agent Card discovery (used by @a2a-js/sdk)
      '/.well-known': 'http://localhost:8080',
      // Health check
      '/health': 'http://localhost:8080',
      // WebSocket approvals (kept for parity with existing UI)
      '/ws': {
        target: 'ws://localhost:8080',
        ws: true,
      },
    },
  },
});
