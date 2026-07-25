import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

export default defineConfig({
  root: ROOT,
  test: {
    include: ['contract-closure/test/**/*.test.ts'],
    environment: 'node',
  },
});
