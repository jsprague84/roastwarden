/**
 * Sign-in (SECURITY.md, "Logins"): what this browser is to the server, the login cookie, the
 * signed-in browsers.
 */

import { BASE_URL, errorMessage, request } from '#lib/api/http.js';

/** Where an older dashboard kept the key. Read once, traded for a login, removed. */
const LEGACY_KEY = 'roastwarden_api_key';

/** A key or token an older dashboard left in local storage: sign in with it once (the server
 * sets its HttpOnly cookie) and remove it, so no credential stays where scripts can read it.
 * Resolves true when it signed in. A refused key is removed too; an unreachable server keeps it. */
export async function adoptStoredKey(): Promise<boolean> {
	let key: string | null;
	try {
		key = localStorage.getItem(LEGACY_KEY);
	} catch {
		return false;
	}
	if (!key) return false;
	try {
		await auth.login(key);
		localStorage.removeItem(LEGACY_KEY);
		return true;
	} catch (e) {
		if (e instanceof LoginRefused) localStorage.removeItem(LEGACY_KEY);
		return false;
	}
}

/** The credential is the owner's: the key, or a browser signed in with it. */
export function isOwnerCredential(c: Credential | undefined | null): boolean {
	return c === 'owner key' || c === 'owner login';
}

/** The key or token was refused at sign-in (401), as opposed to the server not answering. */
export class LoginRefused extends Error {
	override name = 'LoginRefused';
}

export type Credential = 'open' | 'owner key' | 'owner login' | 'token' | 'demo' | 'none';

export type TokenPreset = 'read_only' | 'roast' | 'inventory' | 'admin';

export interface AuthStatus {
	/** Whether the server has an owner key: writes need a credential. */
	auth_enabled: boolean;
	/** Reads need no credential (authentication off, or `ROASTWARDEN_READS=open`). */
	reads_open: boolean;
	/** A public demo (`ROASTWARDEN_DEMO=1`): a visitor with no credential is `demo`. */
	demo: boolean;
	/** A demo that counts its visits in Umami (`ROASTWARDEN_DEMO_STATS_URL`); never on an install that is not a demo. */
	demo_stats: { url: string; website: string } | null;
	/** What this browser is to the server: signed in as the owner (`owner login`), as a token, a demo visitor (`demo`), or `none`. */
	credential: Credential;
	/** This browser's login, when it is signed in. */
	login_id: number | null;
	token: { id: number; name: string; preset: TokenPreset } | null;
}

/** A signed-in browser (Settings → This browser, "Sign-in"). */
export interface Login {
	id: number;
	kind: 'owner' | 'token';
	token_name: string | null;
	created_at: string;
	expires_at: string;
	last_used_at: string | null;
	last_used_ip: string | null;
	user_agent: string | null;
}

export const auth = {
	status: () => request<AuthStatus>('/api/auth/status'),
	/** Sign this browser in with the owner key or a token: the server sets an HttpOnly cookie. */
	login: async (credential: string): Promise<AuthStatus> => {
		const res = await fetch(`${BASE_URL}/api/auth/login`, {
			method: 'POST',
			credentials: 'same-origin',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ credential })
		});
		if (res.status === 401) throw new LoginRefused(errorMessage(await res.text()));
		if (!res.ok) throw new Error(`API error ${res.status}: ${errorMessage(await res.text())}`);
		return res.json();
	},
	/** Sign this browser out. */
	logout: () => request<void>('/api/auth/logout', { method: 'POST' }),
	/** The signed-in browsers (the owner's alone). */
	logins: () => request<Login[]>('/api/auth/logins'),
	endLogin: (id: number) =>
		request<{ ended: number }>(`/api/auth/logins/${id}`, { method: 'DELETE' }),
	endAllLogins: () => request<{ ended: number }>('/api/auth/logins', { method: 'DELETE' })
};
