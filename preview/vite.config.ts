import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  publicDir: '../app/presets',
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  build: { target: 'es2022' },
  test: { environment: 'jsdom' },
});
