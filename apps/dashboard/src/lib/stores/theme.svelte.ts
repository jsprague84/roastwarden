/**
 * Colour theme preference: light, dark, or follow the OS.
 *
 * Stored per browser in localStorage. `src/app.html` applies the same key
 * before the first paint so there is no flash; this store keeps the
 * `data-theme` attribute in sync afterwards and exposes the resolved theme
 * for code (charts) that cannot read CSS variables.
 */

export type ThemePreference = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

const THEME_STORAGE_KEY = 'roastwarden.theme';

function loadPreference(): ThemePreference {
	try {
		if (typeof localStorage !== 'undefined') {
			const v = localStorage.getItem(THEME_STORAGE_KEY);
			if (v === 'light' || v === 'dark' || v === 'system') return v;
		}
	} catch {
		// storage unavailable
	}
	return 'system';
}

function systemTheme(): ResolvedTheme {
	if (
		typeof window !== 'undefined' &&
		window.matchMedia?.('(prefers-color-scheme: light)').matches
	) {
		return 'light';
	}
	return 'dark';
}

export function resolveTheme(pref: ThemePreference, system: ResolvedTheme): ResolvedTheme {
	return pref === 'system' ? system : pref;
}

export const themeState = $state<{ preference: ThemePreference; system: ResolvedTheme }>({
	preference: loadPreference(),
	system: systemTheme()
});

/** The theme actually in effect. Reactive. */
function resolvedTheme(): ResolvedTheme {
	return resolveTheme(themeState.preference, themeState.system);
}

export function isLight(): boolean {
	return resolvedTheme() === 'light';
}

function apply() {
	if (typeof document === 'undefined') return;
	document.documentElement.dataset.theme = resolvedTheme();
}

export function setTheme(pref: ThemePreference) {
	themeState.preference = pref;
	try {
		localStorage.setItem(THEME_STORAGE_KEY, pref);
	} catch {
		// ignore
	}
	apply();
}

export function toggleTheme() {
	setTheme(resolvedTheme() === 'light' ? 'dark' : 'light');
}

let listening = false;

/** Call once from the root layout: applies the theme and tracks OS changes. */
export function initTheme() {
	apply();
	if (listening || typeof window === 'undefined' || !window.matchMedia) return;
	listening = true;
	const mq = window.matchMedia('(prefers-color-scheme: light)');
	mq.addEventListener('change', (e) => {
		themeState.system = e.matches ? 'light' : 'dark';
		apply();
	});
}
