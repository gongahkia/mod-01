import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    emptyOutDir: false,
    lib: { entry: 'src/native-host.ts', formats: ['es'], fileName: () => 'native-host.mjs' },
    minify: false,
    outDir: 'standalone',
    rollupOptions: { external: [/^node:/] },
    sourcemap: false,
  },
});
