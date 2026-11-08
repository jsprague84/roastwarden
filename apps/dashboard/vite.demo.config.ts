/**
 * roastwarden.com's roast demo: src/demo/main.ts, the
 * dashboard's own scope drawing (uPlot, lib/scope) playing a recorded roast, built into
 * site/static/demo/ as one script and one stylesheet with their licence notices. The site's CSP
 * allows only its own files, so nothing is loaded from elsewhere.
 *
 *   npm run build:demo        (the site's build runs it)
 */
import { defineConfig } from 'vite';
import { notices } from './notices.js';

export default defineConfig({
	publicDir: false,
	plugins: [
		notices({
			file: 'roast-demo',
			what: "roastwarden.com's roast demo",
			rest: ['The demo bundles the npm', 'packages below, each under its own licence.']
		})
	],
	build: {
		outDir: '../../site/static/demo',
		emptyOutDir: true,
		target: 'es2022',
		lib: {
			entry: 'src/demo/main.ts',
			formats: ['es'],
			fileName: () => 'roast-demo.js',
			cssFileName: 'roast-demo'
		},
		rolldownOptions: {
			output: {
				// Lib mode keeps an ES bundle's whitespace for other bundlers; this one goes to a browser.
				minify: true,
				// After minifying (a banner would be stripped with the comments)
				postBanner:
					"/*! roastwarden.com's roast demo. Roastwarden: AGPL-3.0-or-later. uPlot: MIT, (c) Leon Sorokin. Notices: /demo/licenses/ */"
			}
		}
	}
});
