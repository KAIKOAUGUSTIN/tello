import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

const root = fileURLToPath(new URL('.', import.meta.url));
const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')) as { version: string };

export default defineConfig({
  root,
  plugins: [svelte()],
  define: { __APP_VERSION__: JSON.stringify(pkg.version) },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // O backend simulado (navegador) lê o vídeo de teste do simulador.
    fs: { allow: [fileURLToPath(new URL('..', import.meta.url))] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  // O worker da IA é um módulo ES (o MediaPipe carrega o wasm com import()).
  worker: { format: 'es' },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
  },
});
