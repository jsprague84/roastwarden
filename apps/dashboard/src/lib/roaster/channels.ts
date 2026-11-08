/**
 * Declared channels (docs/channels.md): the extra values a roaster sends that the owner wants
 * kept with a roast, drawn on the scope and usable in alarms. The server checks the same
 * rules; these keep the Setup panel from offering what it would refuse.
 */
import type { Help } from '#lib/constants/help.js';
import type { RoasterChannel } from '#lib/api/roasters.js';

export const MAX_CHANNELS = 8;

const KEY_RE = /^[A-Za-z][A-Za-z0-9_]{0,31}$/;

/** Sample fields that are not channels: the typed ones, the server's, drum and damper. */
const RESERVED_KEYS = new Set([
	'beanTemp',
	'envTemp',
	'rateOfRise',
	'rateOfRiseEt',
	'deviceRateOfRise',
	'heater',
	'fan',
	'setpoint',
	'controlMode',
	'heaterEnable',
	'timestamp',
	'uptime',
	'uptimeMs',
	'drum',
	'damper'
]);

/** Where a channel is drawn: the temperature axis, the output axis, or a scale of its own. */
export type ChannelAxis = 'temp' | 'pct' | 'own';

export function channelAxis(c: Pick<RoasterChannel, 'unit'>): ChannelAxis {
	return c.unit === 'C' ? 'temp' : c.unit === '%' ? 'pct' : 'own';
}

/** Line colours, distinct from BT, ET, RoR, heater, fan and the setpoint. */
const CHANNEL_COLORS = [
	'#f472b6',
	'#22d3ee',
	'#a3e635',
	'#facc15',
	'#fb7185',
	'#38bdf8',
	'#c4b5fd',
	'#94a3b8'
];

export function channelColor(i: number): string {
	return CHANNEL_COLORS[i % CHANNEL_COLORS.length];
}

/** A channel's value in a sample (the live telemetry or a stored sample's extras). */
export function channelValue(sample: unknown, key: string): number | null {
	if (!sample || typeof sample !== 'object') return null;
	const v = (sample as Record<string, unknown>)[key];
	return typeof v === 'number' && Number.isFinite(v) ? v : null;
}

/** The keys a roaster sends now that could be channels: numeric, not a field of their own. */
export function candidateKeys(telemetry: Record<string, unknown> | null | undefined): string[] {
	if (!telemetry) return [];
	return Object.keys(telemetry)
		.filter((k) => KEY_RE.test(k) && !RESERVED_KEYS.has(k) && channelValue(telemetry, k) != null)
		.sort();
}

/** Why the list would be refused, or null (the server's rule). */
export function channelsProblem(list: RoasterChannel[]): string | null {
	if (list.length > MAX_CHANNELS) return `At most ${MAX_CHANNELS} channels.`;
	const seen = new Set<string>();
	for (const [i, c] of list.entries()) {
		const n = i + 1;
		const key = c.key.trim();
		if (!KEY_RE.test(key))
			return `Channel ${n}: the key must be a letter, then letters, digits or _.`;
		if (RESERVED_KEYS.has(key)) return `Channel ${n}: ${key} is a field of its own, not a channel.`;
		if (seen.has(key)) return `${key} is declared twice.`;
		seen.add(key);
		const l = c.label.trim().length;
		if (l < 1 || l > 24) return `Channel ${n}: the label must be 1 to 24 characters.`;
		const u = c.unit.trim();
		if (u === 'F' || u === '°F') return `Channel ${n}: temperatures are kept in °C; choose °C.`;
		if (u.length < 1 || u.length > 8)
			return `Channel ${n}: the unit must be °C, % or up to 8 characters.`;
		if (c.max != null && !(Number.isFinite(c.max) && c.max > 0))
			return `Channel ${n}: the top of the scale must be above 0.`;
	}
	return null;
}

/** The list as the server takes it: trimmed, `max` only on a scale of its own. */
export function tidyChannels(list: RoasterChannel[]): RoasterChannel[] {
	return list.map((c) => {
		const unit = c.unit.trim() === '°C' ? 'C' : c.unit.trim();
		const own = unit !== 'C' && unit !== '%';
		const max =
			own && c.max != null && Number.isFinite(Number(c.max)) && Number(c.max) > 0
				? Number(c.max)
				: null;
		return { key: c.key.trim(), label: c.label.trim(), unit, max };
	});
}

/** One line: "Exhaust (t3, °C), Gas (burner_kpa, kPa, top 4)". */
export function channelsLine(list: RoasterChannel[]): string {
	if (list.length === 0) return 'None declared: extra values stay live only.';
	return list
		.map(
			(c) =>
				`${c.label} (${c.key}, ${c.unit === 'C' ? '°C' : c.unit}${c.max != null ? `, top ${c.max}` : ''})`
		)
		.join(', ');
}

/** A new channel for a key, with a guess at its unit (a probe name reads as a temperature). */
export function newChannel(key = ''): RoasterChannel {
	const temp = /^(t\d|temp|exhaust|ambient|inlet|outlet|probe)/i.test(key);
	return {
		key,
		label: key ? key.charAt(0).toUpperCase() + key.slice(1) : '',
		unit: temp ? 'C' : '%',
		max: null
	};
}

export const CHANNELS_HELP: Help = {
	line: 'Extra values this roaster sends that Roastwarden keeps with each roast.',
	more: 'A declared channel is stored with the roast, drawn on the scope, the history chart and the roast report, usable in alarms (“channel above / below”) and exported to Artisan as an extra device. °C draws on the temperature axis (in your unit), % on the output axis, any other unit on a scale of its own (0 to its top). Extra values not declared stay on the live view only.'
};
