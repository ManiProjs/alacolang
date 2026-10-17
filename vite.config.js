import { defineConfig } from 'vite';

export default defineConfig({
  base: '/alaco/',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
