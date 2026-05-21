/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import path from 'path'

export default defineConfig({
  resolve: {
    alias: {
      '@shared/brand': path.resolve(__dirname, './brand/index.ts'),
      '@shared/types': path.resolve(__dirname, './types/index.ts'),
      '@shared/api': path.resolve(__dirname, './api'),
    },
  },
  test: {
    environment: 'node',
    globals: true,
    include: ['**/*.{test,spec}.?(c|m)[jt]s?(x)'],
  },
})
