/**
 * Page state kept for this browser tab (sessionStorage), so leaving a page for a moment and
 * coming back finds it as it was: the scope's view, History's sort and filters. A new tab
 * starts fresh. Storage that is blocked or full is ignored.
 */

const PREFIX = 'roastwarden.kept.';

export function kept<T>(key: string, fallback: T): T {
	try {
		const raw = sessionStorage.getItem(PREFIX + key);
		return raw == null ? fallback : (JSON.parse(raw) as T);
	} catch {
		return fallback;
	}
}

export function keep(key: string, value: unknown): void {
	try {
		sessionStorage.setItem(PREFIX + key, JSON.stringify(value));
	} catch {
		/* private mode or full: the page still works, it just forgets */
	}
}
