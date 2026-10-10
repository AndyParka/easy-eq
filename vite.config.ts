import { defineConfig } from 'vite';
export default defineConfig({
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  clearScreen: false,
  // Rolldown's HTML scanner treats # in this existing workspace path as a URL fragment.
  // All dependencies are small ESM modules; serve them directly in development.
  optimizeDeps: { noDiscovery: true, include: [] },
});
