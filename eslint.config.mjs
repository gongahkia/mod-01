import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';

const typedRules = tseslint.configs.strictTypeChecked.map((config) => ({
  ...config,
  files: ['**/*.ts'],
}));

export default tseslint.config(
  {
    ignores: [
      '**/dist/**',
      '**/coverage/**',
      '**/node_modules/**',
      'packages/runtime/standalone/**',
      'packages/runtime/vite.*.config.ts',
    ],
  },
  eslint.configs.recommended,
  ...typedRules,
  {
    files: ['**/*.ts'],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
);
