import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';
import importX from 'eslint-plugin-import-x';
import eslintConfigPrettier from 'eslint-config-prettier';
import { builtinModules } from 'node:module';
import { defineConfig } from 'eslint/config';

export default defineConfig(
  eslint.configs.recommended,
  eslintConfigPrettier,
  ...tseslint.configs.stylisticTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        project: ['./tsconfig.json'],
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  {
    files: ['**/*.{ts,tsx,js,mjs}'],
    plugins: {
      'import-x': importX,
    },
    rules: {
      'no-debugger': 'error',
      'no-console': ['error', { allow: ['warn', 'error', 'info'] }],
      'no-unused-vars': 'off',
      '@typescript-eslint/no-unused-vars': 'error',
      'prefer-template': 'error',
      'sort-imports': ['error', { ignoreDeclarationSort: true }],
      '@typescript-eslint/no-floating-promises': ['error'],
      '@typescript-eslint/no-explicit-any': ['error'],
      // '@typescript-eslint/explicit-module-boundary-types': 'off',
      '@typescript-eslint/prefer-optional-chain': 'error',
      'import-x/no-nodejs-modules': [
        'error',
        { allow: builtinModules.map((mod) => `node:${mod}`) },
      ],
      // Enforce the use of 'import type' for importing types
      '@typescript-eslint/consistent-type-imports': [
        'error',
        {
          fixStyle: 'inline-type-imports',
          disallowTypeAnnotations: false,
        },
      ],
      // Enforce the use of top-level import type qualifier when an import only has specifiers with inline type qualifiers
      '@typescript-eslint/no-import-type-side-effects': 'error',
    },
  },
  // Node scripts
  {
    files: ['script/**', '*.{js,mjs,ts}'],
    rules: {
      'no-console': 'off',
    },
  },
  {
    files: ['**/*.{js,mjs}'],
    ...tseslint.configs.disableTypeChecked,
  },
  // ignores
  {
    ignores: [
      '**/node_modules/',
      '**/dist/',
      '**/temp/',
      '**/coverage/',
      'deps/',
      '.github/',
      '.husky/',
      '.idea/',
      '.vscode/',
      'src-tauri/',
      'src-proxy/',
      'src-server/',
      'src-common/',
      'src-vpn/',
      'tauri-plugin-ios/',
    ],
  },
);
