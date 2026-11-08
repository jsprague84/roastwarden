/**
 * Settings → Home Assistant (docs/home-assistant.md): the form and the settings it saves.
 * The password is a secret: sent empty, the stored one stays.
 */
import type { HaStatus } from '#lib/api/homeAssistant.js';

export interface HaForm {
	enabled: boolean;
	/** 'server': the server's own broker; 'other': Home Assistant's own */
	where: 'server' | 'other';
	broker: string;
	username: string;
	password: string;
	prefix: string;
	interval: number;
	includeVirtual: boolean;
}

export function fromStatus(s: HaStatus, stored: Record<string, string>): HaForm {
	const broker = (stored.ha_broker ?? '').trim();
	return {
		enabled: s.enabled,
		where: broker ? 'other' : 'server',
		broker: broker || 'mqtt://',
		username: s.username ?? stored.ha_username ?? '',
		password: '',
		prefix: s.prefix || 'homeassistant',
		interval: s.interval_seconds || 5,
		includeVirtual: s.include_virtual
	};
}

/** Why the form cannot be saved yet, or null */
export function problem(f: HaForm): string | null {
	if (f.where === 'other' && !/^mqtt:\/\/[^\s/@:]+(:\d{1,5})?\/?$/.test(f.broker.trim()))
		return "Home Assistant's broker is mqtt://host or mqtt://host:port";
	if (!/^[A-Za-z0-9_-]+(\/[A-Za-z0-9_-]+)*$/.test(f.prefix.trim()))
		return 'The prefix is a topic such as homeassistant';
	if (!Number.isInteger(f.interval) || f.interval < 1 || f.interval > 60)
		return 'The interval is 1 to 60 s';
	return null;
}

export function values(f: HaForm): Record<string, string> {
	const other = f.where === 'other';
	return {
		ha_enabled: String(f.enabled),
		ha_broker: other ? f.broker.trim() : '',
		ha_username: other ? f.username.trim() : '',
		// Empty keeps the stored password
		ha_password: other ? f.password : '',
		ha_prefix: f.prefix.trim(),
		ha_interval_seconds: String(f.interval),
		ha_virtual: String(f.includeVirtual)
	};
}

/** The status in one line */
export function statusLine(s: HaStatus): { ok: boolean; text: string } {
	if (!s.enabled) return { ok: true, text: 'Off: nothing is published.' };
	if (s.problem) return { ok: false, text: s.problem };
	const where = s.broker === 'server' ? "the server's broker" : s.broker;
	const n = s.roasters.length;
	const waiting = s.waiting.length
		? ` Not yet: ${s.waiting.join(', ')}, waiting to be set up under Roasters.`
		: '';
	return {
		ok: true,
		text: `Publishing to ${where}: ${n === 0 ? 'no roaster' : `${n} roaster${n === 1 ? '' : 's'}`}.${waiting}`
	};
}
