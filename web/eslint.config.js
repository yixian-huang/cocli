import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import tseslint from 'typescript-eslint'
import { defineConfig, globalIgnores } from 'eslint/config'
import { fileURLToPath } from 'url'
import path from 'path'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(__dirname, '..')

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    rules: {
      'react-refresh/only-export-components': 'off',
      'react-hooks/set-state-in-effect': 'off',
    },
  },
  // Enforce platform-agnostic purity in shared/ (no react, react-dom, react-native)
  {
    basePath: repoRoot,
    files: ['shared/**/*.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          paths: [
            { name: 'react', message: 'shared/ must be platform-agnostic.' },
            { name: 'react-dom', message: 'shared/ must be platform-agnostic.' },
            { name: 'react-native', message: 'shared/ must be platform-agnostic.' },
          ],
          patterns: [
            { group: ['react-native*'], message: 'shared/ must be platform-agnostic.' },
            { group: ['@react-native*'], message: 'shared/ must be platform-agnostic.' },
          ],
        },
      ],
    },
  },
])
