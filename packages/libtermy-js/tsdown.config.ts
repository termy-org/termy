import { defineConfig } from 'tsdown'

export default defineConfig({
  // We emit the Web Worker entry as a top-level `worker.js` next to
  // `index.js` so the workerized bridge can resolve it via
  // `new Worker(new URL('./worker.js', import.meta.url), { type: 'module' })`
  // at runtime. Named entry keys ensure the output filenames are predictable
  // (no hash, no nested folders) — important because the bridge constructs
  // the URL by string literal.
  entry: {
    index: './src/index.ts',
    worker: './src/renderer/worker/worker.ts',
  },
  format: ['esm'],
  platform: 'browser',
  target: 'es2022',
  dts: true,
  clean: true,
})
