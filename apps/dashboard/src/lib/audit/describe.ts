/**
 * Plain words for an audit-log entry (Settings → Audit): what the request asked for, from
 * its method, path and body. Unknown requests fall back to the method and path.
 */

export interface AuditEntry {
	id: number;
	at: string;
	actor: string;
	token_id: number | null;
	token_name: string | null;
	remote_user: string | null;
	client: string | null;
	ip: string | null;
	method: string;
	path: string;
	device_id: string | null;
	status: number;
	body: string | null;
	/** From the path on the server (audit.rs `KINDS`) */
	kind: string;
}

/** The server's audit kinds, in the order the filter shows them. */
export const AUDIT_KINDS: { id: string; label: string }[] = [
	{ id: 'control', label: 'Control' },
	{ id: 'roast', label: 'Roasts' },
	{ id: 'profile', label: 'Profiles' },
	{ id: 'beans', label: 'Beans and plan' },
	{ id: 'labels', label: 'Labels' },
	{ id: 'roaster', label: 'Roaster setup' },
	{ id: 'settings', label: 'Settings' },
	{ id: 'access', label: 'Tokens and approvals' },
	{ id: 'backup', label: 'Backups' },
	{ id: 'other', label: 'Other' }
];

const CONTROL: Record<string, string> = {
	setpoint: 'Setpoint',
	heater: 'Heater',
	fan: 'Fan',
	mode: 'Mode',
	heater_enable: 'Heater power',
	pid: 'PID gains',
	'pid/undo': 'Undo PID change',
	ror_limit: 'RoR limit',
	emergency_stop: 'Emergency stop'
};

const SESSION: Record<string, string> = {
	start: 'Start roast',
	pause: 'Pause roast',
	resume: 'Resume roast',
	complete: 'Complete roast',
	cancel: 'Cancel roast',
	'drive/resume': 'Resume Profile drive',
	replay: 'Profile drive',
	'replay/pause': 'Pause Profile drive',
	'replay/resume': 'Resume Profile drive',
	follow: 'Follow profile',
	events: 'Mark event',
	profile: 'Save roast as profile'
};

function parse(body: string | null): Record<string, unknown> | null {
	if (!body) return null;
	try {
		const v = JSON.parse(body);
		return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
	} catch {
		return null;
	}
}

function valueOf(b: Record<string, unknown> | null): string {
	if (!b) return '';
	if (typeof b.percent === 'number') return `${b.percent} %`;
	if (typeof b.enabled === 'boolean') return b.enabled ? 'on' : 'off';
	if (typeof b.kp === 'number') return `Kp ${b.kp} · Ki ${b.ki} · Kd ${b.kd}`;
	if (typeof b.max === 'number') return b.max === 0 ? 'off' : `${b.max} °C/min`;
	for (const k of ['value', 'mode', 'event_type']) {
		if (b[k] !== undefined && b[k] !== null) return String(b[k]);
	}
	return '';
}

/** What the request asked for, in a few words. */
export function describe(e: Pick<AuditEntry, 'method' | 'path' | 'body'>): string {
	const b = parse(e.body);
	const p = e.path.replace(/\/+$/, '');
	let m = p.match(/^\/api\/roaster\/[^/]+\/control\/(.+)$/);
	if (m) {
		const label = CONTROL[m[1]] ?? `Control ${m[1]}`;
		const v = valueOf(b);
		return v ? `${label} ${v}` : label;
	}
	m = p.match(/^\/api\/sessions\/[^/]+\/(.+)$/);
	if (m) {
		const what = SESSION[m[1]] ?? m[1].replace(/\//g, ' ');
		if (e.method === 'DELETE') return `Stop ${what.toLowerCase()}`;
		const v = m[1] === 'events' ? valueOf(b).toUpperCase() : '';
		return v ? `${what} ${v}` : what;
	}
	m = p.match(/^\/api\/virtual-roaster\/(charge|dump)$/);
	if (m) return m[1] === 'charge' ? 'Simulator: charge beans' : 'Simulator: dump beans';
	if (p === '/api/sessions' && e.method === 'POST')
		return b?.name ? `New roast “${b.name}”` : 'New roast';
	m = p.match(/^\/api\/sessions\/[^/]+$/);
	if (m) return e.method === 'DELETE' ? 'Delete roast' : 'Edit roast details';
	if (p === '/api/tokens' && e.method === 'POST') {
		const preset = typeof b?.preset === 'string' ? ` (${b.preset.replace('_', '-')})` : '';
		return b?.name ? `New token “${b.name}”${preset}` : 'New token';
	}
	if (/^\/api\/tokens\/[^/]+\/revoke$/.test(p)) return 'Revoke token';
	if (/^\/api\/tokens\/[^/]+$/.test(p) && e.method === 'DELETE') return 'Remove token';
	if (/^\/api\/tokens\/[^/]+$/.test(p) && e.method === 'PUT') {
		return `Token asks first: ${b?.require_approval === true ? 'on' : b?.require_approval === false ? 'off' : '?'}`;
	}
	m = p.match(/^\/api\/approvals\/[^/]+\/(approve|deny)$/);
	if (m) return m[1] === 'approve' ? 'Approve a request' : 'Deny a request';
	if (/^\/api\/roaster\/[^/]+\/token-access$/.test(p)) {
		return `Tokens may control this roaster: ${b?.tokens_may_control === true ? 'on' : b?.tokens_may_control === false ? 'off' : '?'}`;
	}
	m = p.match(/^\/api\/settings\/(.+)$/);
	if (m) {
		// Long values (the alarm rules are a JSON list) stay readable in the row; the body has them whole.
		const v = b && 'value' in b ? String(b.value) : '';
		return `Setting ${decodeURIComponent(m[1])}${v ? ` = ${v.length > 60 ? `${v.slice(0, 60)}…` : v}` : ''}`;
	}
	return `${e.method} ${p.replace(/^\/api/, '')}`;
}

/** Who sent it, in a few words. */
export function who(e: Pick<AuditEntry, 'actor' | 'token_name' | 'remote_user'>): string {
	const cred =
		e.actor === 'token' && e.token_name
			? `token “${e.token_name}”`
			: e.actor === 'none'
				? e.token_name
					? `token “${e.token_name}” (no longer valid)`
					: 'no key'
				: e.actor;
	return e.remote_user ? `${e.remote_user} · ${cred}` : cred;
}

/** A short name for a browser or client from its User-Agent. */
export function clientName(ua: string | null): string {
	if (!ua) return '';
	if (/curl\//i.test(ua)) return 'curl';
	const browser = /Edg\//.test(ua)
		? 'Edge'
		: /Firefox\//.test(ua)
			? 'Firefox'
			: /Chrome\//.test(ua)
				? 'Chrome'
				: /Safari\//.test(ua)
					? 'Safari'
					: '';
	const os = /iPhone|iPad/.test(ua)
		? 'iOS'
		: /Android/.test(ua)
			? 'Android'
			: /Mac OS X/.test(ua)
				? 'macOS'
				: /Windows/.test(ua)
					? 'Windows'
					: /Linux/.test(ua)
						? 'Linux'
						: '';
	if (browser) return os ? `${browser} on ${os}` : browser;
	return ua.split(/[\s(]/)[0];
}
