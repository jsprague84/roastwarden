/** A token's state in words for Settings → Tokens. */
import type { TokenPreset } from '#lib/api/auth.js';
import type { ApiToken } from '#lib/api/tokens.js';

export const PRESETS: { id: TokenPreset; label: string; line: string }[] = [
	{
		id: 'read_only',
		label: 'Read-only',
		line: 'Reads roasts, profiles, beans, telemetry and settings. Changes nothing.'
	},
	{
		id: 'roast',
		label: 'Roast',
		line: 'Reads, and runs a roast: start, events, Profile drive, setpoint, fan, heater, emergency stop, cupping, the plan.'
	},
	{
		id: 'inventory',
		label: 'Inventory',
		line: 'Reads, and keeps the green coffee: beans, stock purchases and corrections, the roast plan. Cannot touch a roaster.'
	},
	{
		id: 'admin',
		label: 'Admin',
		line: 'Everything except managing tokens, the per-roaster switch and restoring a backup, which stay with the owner key.'
	}
];

export const presetLabel = (p: string) => PRESETS.find((x) => x.id === p)?.label ?? p;

export type TokenState = { kind: 'active' | 'expired' | 'revoked'; text: string };

const DAY = 86_400_000;

function days(ms: number): string {
	const d = Math.round(ms / DAY);
	return d <= 0 ? 'today' : d === 1 ? 'in 1 day' : `in ${d} days`;
}

export function tokenState(
	t: Pick<ApiToken, 'expires_at' | 'revoked_at'>,
	now = Date.now()
): TokenState {
	if (t.revoked_at) return { kind: 'revoked', text: 'Revoked' };
	if (t.expires_at) {
		const left = Date.parse(t.expires_at) - now;
		if (!(left > 0)) return { kind: 'expired', text: 'Expired' };
		return { kind: 'active', text: `Expires ${days(left)}` };
	}
	return { kind: 'active', text: 'Never expires' };
}

/** "never", or how long ago, for the last-used column. */
export function ago(iso: string | null, now = Date.now()): string {
	if (!iso) return 'never';
	const s = Math.max(0, Math.round((now - Date.parse(iso)) / 1000));
	if (s < 90) return 'just now';
	const m = Math.round(s / 60);
	if (m < 90) return `${m} min ago`;
	const h = Math.round(m / 60);
	if (h < 36) return `${h} h ago`;
	return `${Math.round(h / 24)} days ago`;
}

/** A signed-in browser's name from its user agent, roughly: "Firefox on Linux", "Safari on iPhone". */
export function browserName(ua: string | null): string {
	if (!ua) return 'A browser';
	const browser = /Edg\//.test(ua)
		? 'Edge'
		: /Firefox\/|FxiOS\//.test(ua)
			? 'Firefox'
			: /Chrome\/|CriOS\//.test(ua)
				? 'Chrome'
				: /Safari\//.test(ua)
					? 'Safari'
					: 'A browser';
	const system = /iPhone/.test(ua)
		? 'iPhone'
		: /iPad/.test(ua)
			? 'iPad'
			: /Android/.test(ua)
				? 'Android'
				: /Mac OS X|Macintosh/.test(ua)
					? 'macOS'
					: /Windows/.test(ua)
						? 'Windows'
						: /Linux/.test(ua)
							? 'Linux'
							: '';
	return system ? `${browser} on ${system}` : browser;
}

/**
 * What to run on the agent's machine after making its token (docs/agents.md, "Setting up an
 * agent"): `roast login` asks for the token and keeps it, then the skill goes where the agent
 * reads skills. `origin` is this server's address as the browser reaches it.
 */
export function agentSetup(origin: string): string {
	const url = origin.replace(/\/+$/, '');
	return [
		`roast login ${url}            # paste the token when asked`,
		'roast skill install claude-code   # or: hermes, agents (Codex and others)',
		'roast status                      # the agent can use roast now'
	].join('\n');
}
