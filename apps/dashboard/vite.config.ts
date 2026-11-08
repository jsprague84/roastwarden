import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';
import pkg from './package.json' with { type: 'json' };
import { notices } from './notices.js';

// The commit CI builds (the Dockerfile's ROASTWARDEN_REVISION); no Node types in this project
const env =
	(globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};

export default defineConfig({
	// Settings → About shows them next to the server's: the version, and the revision (CI's
	// build-arg) to tell an open tab that is older than the server.
	define: {
		__APP_VERSION__: JSON.stringify(pkg.version),
		__APP_REVISION__: JSON.stringify(env.ROASTWARDEN_REVISION || 'dev')
	},
	plugins: [
		tailwindcss(),
		// build/licenses/: the bundle's npm licences, and a licence check (notices.js)
		notices(),
		// SvelteKit 3: the configuration lives here (svelte.config.js is gone).
		sveltekit({
			adapter: adapter({
				pages: 'build',
				assets: 'build',
				fallback: 'index.html', // SPA mode: client-side routing for every path
				precompress: false,
				strict: true
			})
		})
	],
	server: {
		proxy: {
			'/api': 'http://localhost:8080',
			'/ws': {
				target: 'http://localhost:8080',
				ws: true
			},
			'/healthz': 'http://localhost:8080',
			'/readyz': 'http://localhost:8080'
		}
	}
});
