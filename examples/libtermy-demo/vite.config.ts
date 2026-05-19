import { defineConfig } from 'vite'

export default defineConfig({
  server: {
    port: 5173,
    strictPort: false,
  },
  build: {
    target: 'es2022',
  },
  optimizeDeps: {
    // libtermy.js embeds its WASM bytes; let Vite leave it alone.
    exclude: ['libtermy.js'],
  },
})
