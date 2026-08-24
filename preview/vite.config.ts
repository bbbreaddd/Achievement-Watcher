import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

const sourceRoot = fileURLToPath(new URL('.', import.meta.url));
const originalAppAssets = fileURLToPath(new URL('../app', import.meta.url));

export default defineConfig({
  plugins: [svelte()],
  publicDir: '../app/presets',
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    fs: { allow: [sourceRoot, originalAppAssets] },
  },
  build: { target: 'es2022' },
  test: { environment: 'jsdom' },
});
