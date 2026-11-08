// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
declare global {
	/** The dashboard's version (package.json), set by vite.config.ts. */
	const __APP_VERSION__: string;
	/** The git commit this dashboard was built from (CI), or `dev` */
	const __APP_REVISION__: string;

	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};
