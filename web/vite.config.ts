/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      '@shared/brand': path.resolve(__dirname, '../shared/brand/index.ts'),
      '@shared/types': path.resolve(__dirname, '../shared/types/index.ts'),
      '@shared/api': path.resolve(__dirname, '../shared/api'),
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: [
      'src/**/*.{test,spec}.?(c|m)[jt]s?(x)',
      '../shared/**/*.{test,spec}.?(c|m)[jt]s?(x)',
    ],
    exclude: [
      '**/node_modules/**',
      'node_modules/**',
      '../shared/node_modules/**',
    ],
  },
  server: {
    // 5173 is often taken by other Vite apps on the same machine.
    port: 8091,
    strictPort: true,
    proxy: {
      '/api': 'http://localhost:8090',
      '/ws': {
        target: 'ws://localhost:8090',
        ws: true,
      },
    },
  },
})
