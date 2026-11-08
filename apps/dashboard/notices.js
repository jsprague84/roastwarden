// @ts-nocheck: build tooling run by Node, and the project has no Node types
/**
 * Third-party notices for the dashboard (docs/DEVELOPMENT.md, "Licences"). After a client
 * build: every npm package whose code, styles or fonts are in the bundle, with its licence and
 * NOTICE files, in build/licenses/dashboard.txt; Roastwarden's own LICENSE beside it as
 * LICENSE.txt. Settings → About links both. A bundled package under a licence not in ALLOWED
 * fails the build: check it is compatible with AGPL-3.0-or-later before adding it.
 * (Vite's own build.license skips fonts and NOTICE files and checks nothing.)
 */
import fs from 'node:fs';
import path from 'node:path';

const ALLOWED = new Set([
	'MIT',
	'ISC',
	'Apache-2.0',
	'BSD-2-Clause',
	'BSD-3-Clause',
	'0BSD',
	'CC0-1.0',
	'Unlicense',
	'Zlib',
	'MPL-2.0',
	'OFL-1.1'
]);
const TEXT = /^(licen[cs]e|copying|notice)/i;
const RULE = '-'.repeat(96);

/** "(MIT OR Apache-2.0)": one alternative whose every part is allowed. */
function allowed(expr) {
	if (!expr) return false;
	return expr
		.replace(/[()]/g, '')
		.split(/\s+OR\s+/)
		.some((alt) => alt.split(/\s+AND\s+/).every((id) => ALLOWED.has(id.trim())));
}

/** The nearest package.json above a file in node_modules, with a name. */
function packageOf(file, cache) {
	let dir = path.dirname(file);
	while (dir.includes(`${path.sep}node_modules${path.sep}`)) {
		if (cache.has(dir)) return cache.get(dir);
		const manifest = path.join(dir, 'package.json');
		if (fs.existsSync(manifest)) {
			const data = JSON.parse(fs.readFileSync(manifest, 'utf-8'));
			if (data.name) {
				const pkg = { dir, data };
				cache.set(dir, pkg);
				return pkg;
			}
		}
		dir = path.dirname(dir);
	}
	return null;
}

function licence(data) {
	if (typeof data.license === 'string') return data.license.trim();
	if (data.license?.type) return data.license.type;
	if (Array.isArray(data.licenses)) return data.licenses.map((l) => l.type ?? l).join(' OR ');
	return null;
}

/**
 * `file`: the notices' name under licenses/; `what`: the bundle, for their heading; `rest`: the
 * sentence after "… (LICENSE.txt next to this file)." (roastwarden.com's roast demo has its own).
 */
export function notices({
	file = 'dashboard',
	what = 'the Roastwarden dashboard',
	rest = [
		'The dashboard bundles the npm',
		"packages below, each under its own licence; the server's crates are in server.txt."
	]
} = {}) {
	let root = process.cwd();
	return {
		name: 'roastwarden:notices',
		apply: 'build',
		configResolved(config) {
			root = config.root;
		},
		generateBundle(_, bundle) {
			if (this.environment && this.environment.name !== 'client') return;
			const files = new Set();
			for (const out of Object.values(bundle)) {
				if (out.type === 'chunk')
					for (const id of out.moduleIds ?? Object.keys(out.modules ?? {})) files.add(id);
				else for (const f of out.originalFileNames ?? []) files.add(path.resolve(root, f));
			}
			const cache = new Map();
			const packages = new Map();
			for (const id of files) {
				if (id.startsWith('\0')) continue;
				const file = id.split('?')[0];
				if (!file.includes(`${path.sep}node_modules${path.sep}`)) continue;
				const pkg = packageOf(file, cache);
				if (pkg) packages.set(`${pkg.data.name}@${pkg.data.version}`, pkg);
			}
			const refused = [];
			const entries = [...packages.values()]
				.sort((a, b) => a.data.name.localeCompare(b.data.name))
				.map(({ dir, data }) => {
					const id = licence(data);
					if (!allowed(id)) refused.push(`${data.name}@${data.version} (${id ?? 'no licence'})`);
					const texts = fs
						.readdirSync(dir)
						.filter((f) => TEXT.test(f) && fs.statSync(path.join(dir, f)).isFile())
						.sort()
						.map((f) => fs.readFileSync(path.join(dir, f), 'utf-8').trim());
					const home = data.homepage ?? data.repository?.url ?? data.repository ?? '';
					return [
						RULE,
						`${data.name} ${data.version} (${id})${home ? `: ${home}` : ''}`,
						'',
						...texts.flatMap((t) => [t, ''])
					].join('\n');
				});
			if (refused.length) {
				this.error(
					`npm packages in the bundle under a licence not allowed (notices.js): ${refused.join(', ')}`
				);
			}
			const head = [
				`Third-party notices: ${what}`,
				'',
				'Roastwarden is free software under the GNU Affero General Public License, version 3 or',
				`any later version (LICENSE.txt next to this file). ${rest[0]}`,
				...rest.slice(1),
				'',
				`${packages.size} packages:`,
				...[...packages.values()]
					.map(({ data }) => `  ${data.name} ${data.version} (${licence(data)})`)
					.sort(),
				'',
				''
			];
			this.emitFile({
				type: 'asset',
				fileName: `licenses/${file}.txt`,
				source: head.join('\n') + entries.join('\n')
			});
			// Roastwarden's own licence (the Docker build copies it to /app/LICENSE for this)
			const own = path.resolve(root, '../../LICENSE');
			if (fs.existsSync(own))
				this.emitFile({
					type: 'asset',
					fileName: 'licenses/LICENSE.txt',
					source: fs.readFileSync(own, 'utf-8')
				});
		}
	};
}
