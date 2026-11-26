import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

import svelteConfig from './svelte.config.js';

export default defineConfig(
  {
    ignores: ['dist/', 'src/engine/wasm/', 'test-results/', 'playwright-report/']
  },
  js.configs.recommended,
  ts.configs.strictTypeChecked,
  ts.configs.stylisticTypeChecked,
  svelte.configs.recommended,
  {
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        projectService: true,
        extraFileExtensions: ['.svelte'],
        tsconfigRootDir: import.meta.dirname
      }
    }
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts'],
    languageOptions: {
      parserOptions: { parser: ts.parser, svelteConfig }
    }
  },
  {
    files: ['*.config.{js,ts}', 'tests/**', 'src/**/*.test.ts'],
    languageOptions: { globals: globals.node }
  },
  {
    files: ['*.config.js', '.dependency-cruiser.cjs'],
    extends: [ts.configs.disableTypeChecked],
    languageOptions: { globals: globals.node }
  }
);
