// ESLint (`npm run lint`): the recommended rules of ESLint, typescript-eslint and
// eslint-plugin-svelte, without type information (fast; svelte-check does the type checking).
// eslint-config-prettier last: Prettier owns formatting (`npm run format`).
import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import svelte from 'eslint-plugin-svelte';
import { defineConfig, globalIgnores } from 'eslint/config';
import globals from 'globals';
import ts from 'typescript-eslint';

export default defineConfig(
	globalIgnores(['build/', '.svelte-kit/', 'coverage/', 'node_modules/']),
	js.configs.recommended,
	ts.configs.recommended,
	svelte.configs.recommended,
	prettier,
	svelte.configs.prettier,
	{
		languageOptions: {
			globals: { ...globals.browser },
			parserOptions: { extraFileExtensions: ['.svelte'] }
		},
		rules: {
			// TypeScript and svelte-check already report undefined names, with the types' globals
			// (typescript-eslint's advice for TypeScript projects).
			'no-undef': 'off',
			// Svelte drops the whitespace at a block's edges; `{' · '}` after `{#if …}` keeps it.
			'svelte/no-useless-mustaches': 'off',
			// `_` names a value that is there for its position (an `{#each}` item, a parameter).
			'@typescript-eslint/no-unused-vars': [
				'error',
				{ argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' }
			],
			// A file-wide `@ts-nocheck` says why (notices.js: Node code, no Node types here).
			'@typescript-eslint/ban-ts-comment': ['error', { 'ts-nocheck': 'allow-with-description' }]
		}
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
		languageOptions: { parserOptions: { parser: ts.parser } }
	},
	{
		// Node: the build's configs and the licence plugin.
		files: ['*.config.ts', '*.config.js', 'notices.js'],
		languageOptions: { globals: { ...globals.node } }
	}
);
