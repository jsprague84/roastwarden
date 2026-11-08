import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
	plugins: [svelte()],
	test: {
		environment: 'jsdom',
		// `npm run coverage`: every source file counts, tested or not, so the number shows what
		// the tests leave out (the components are covered by the browser checks instead).
		coverage: {
			provider: 'v8',
			include: ['src/**/*.{ts,svelte}'],
			exclude: ['src/**/*.test.ts', 'src/**/*.d.ts', 'src/demo/**', 'src/service-worker/**'],
			reporter: ['text-summary', 'json-summary', 'html'],
			reportsDirectory: 'coverage'
		}
	},
	// The server's golden WebSocket v2 frame lives with the server's tests (paths are relative
	// to this folder).
	server: { fs: { allow: ['.', '../../crates/server/testdata'] } },
	resolve: {
		conditions: ['browser']
	}
});
