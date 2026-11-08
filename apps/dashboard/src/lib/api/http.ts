/**
 * The request core: the server's address, the JSON request and its error classes, a file
 * downloaded, and the helpers the area modules share.
 */

export const BASE_URL = import.meta.env.VITE_API_URL ?? '';

/** An error's text: the API's JSON body (`{success: false, error: {code, message}}`,
 * `crates/server/src/http/errors.rs`), or plain text from a proxy in front of it. */
export function errorMessage(body: string): string {
	try {
		const v = JSON.parse(body) as { error?: { message?: unknown } };
		if (typeof v?.error?.message === 'string') return v.error.message;
	} catch {
		/* not JSON */
	}
	return body.trim();
}

/** An error body's code (`{error: {code}}`), or null. */
function errorCode(body: string): string | null {
	try {
		const v = JSON.parse(body) as { error?: { code?: unknown } };
		if (typeof v?.error?.code === 'string') return v.error.code;
	} catch {
		/* not JSON */
	}
	return null;
}

/** A 409 `control_off`: the roaster's setup does not let Roastwarden send that
 * (docs/roaster-setup.md). Its message is the server's, without the status. */
export class ControlOffError extends Error {
	override name = 'ControlOffError';
}

/** A 409 `already_marked`: the roast has its CHARGE or DROP already (an auto DROP this page
 * missed while hidden). */
export class AlreadyMarkedError extends Error {
	override name = 'AlreadyMarkedError';
}

export async function request<T>(
	path: string,
	options: RequestInit = {},
	// Kept for call-site readability; the browser's login cookie goes with every request
	_requiresAuth = false
): Promise<T> {
	const headers: Record<string, string> = {
		'Content-Type': 'application/json',
		...((options.headers as Record<string, string>) ?? {})
	};

	const res = await fetch(`${BASE_URL}${path}`, {
		...options,
		credentials: 'same-origin',
		headers
	});

	if (res.status === 401) {
		window.dispatchEvent(new CustomEvent('auth_required'));
		throw new Error('Authentication required');
	}

	if (!res.ok) {
		const body = await res.text();
		// The roaster's setup refused it (control level or an undeclared output): its own words.
		const code = errorCode(body)?.toLowerCase();
		if (code === 'control_off' || code === 'output_not_declared')
			throw new ControlOffError(errorMessage(body));
		if (code === 'already_marked') throw new AlreadyMarkedError(errorMessage(body));
		throw new Error(`API error ${res.status}: ${errorMessage(body)}`);
	}

	if (res.status === 204) return undefined as T;
	return res.json();
}

export async function downloadFile(path: string, filename: string): Promise<void> {
	const res = await fetch(`${BASE_URL}${path}`, { credentials: 'same-origin' });
	if (res.status === 401) {
		window.dispatchEvent(new CustomEvent('auth_required'));
		throw new Error('Authentication required');
	}
	if (!res.ok) {
		const why = errorMessage(await res.text().catch(() => ''));
		throw new Error(why && why.length < 200 ? why : `Export failed: ${res.status}`);
	}

	const blob = await res.blob();
	const url = URL.createObjectURL(blob);
	const a = document.createElement('a');
	a.href = url;
	a.download = filename;
	a.click();
	URL.revokeObjectURL(url);
}

/** The browser's offset from UTC, minutes east: the page's dates are the owner's. */
export function tzMinutes(): number {
	return -new Date().getTimezoneOffset();
}

/** A roaster's own path: `/api/roaster/<id>`. */
export const roasterPath = (id: string) => `/api/roaster/${encodeURIComponent(id)}`;
