/** Comparator helpers (docs/analysis.md). Times in seconds, temperatures °C. */
import type { RoastEvent } from '#lib/api/events.js';
import type { SessionTelemetryPoint } from '#lib/api/sessions.js';

export type AlignOn = 'charge' | 'turning_point' | 'drying_end' | 'first_crack_start' | 'drop';

export const ALIGN_OPTIONS: { value: AlignOn; label: string }[] = [
	{ value: 'charge', label: 'CHARGE' },
	{ value: 'turning_point', label: 'Turning point' },
	{ value: 'drying_end', label: 'Dry end' },
	{ value: 'first_crack_start', label: 'First crack' },
	{ value: 'drop', label: 'Drop' }
];

/**
 * Session-elapsed second that becomes t = 0: the landmark, else CHARGE,
 * else the session start. `missing` is true when the landmark was absent.
 */
export function alignOffset(
	events: RoastEvent[],
	on: AlignOn
): { offset: number; missing: boolean } {
	const at = (t: string) => events.find((e) => e.event_type === t)?.elapsed_seconds;
	const hit = at(on);
	if (hit != null) return { offset: hit, missing: false };
	return { offset: at('charge') ?? 0, missing: on !== 'charge' };
}

/** BT samples on the aligned axis. */
export function alignedBt(telemetry: SessionTelemetryPoint[], offset: number): [number, number][] {
	return telemetry
		.filter((p) => p.bean_temp != null)
		.map((p) => [p.elapsed_seconds - offset, p.bean_temp as number]);
}

/** Linear value at `t` on sorted points; null outside them. */
export function interp(pts: [number, number][], t: number): number | null {
	if (!pts.length || t < pts[0][0] || t > pts[pts.length - 1][0]) return null;
	let lo = 0;
	let hi = pts.length - 1;
	while (hi - lo > 1) {
		const mid = (lo + hi) >> 1;
		if (pts[mid][0] <= t) lo = mid;
		else hi = mid;
	}
	const [t0, v0] = pts[lo];
	const [t1, v1] = pts[hi];
	return t1 === t0 ? v0 : v0 + ((v1 - v0) * (t - t0)) / (t1 - t0);
}

/** `other − reference` BT every `step` seconds where both exist, within `from..to`. */
export function deltaToReference(
	ref: [number, number][],
	other: [number, number][],
	from: number,
	to: number,
	step = 5
): [number, number][] {
	const out: [number, number][] = [];
	for (let t = Math.ceil(from / step) * step; t <= to; t += step) {
		const a = interp(other, t);
		const b = interp(ref, t);
		if (a != null && b != null) out.push([t, a - b]);
	}
	return out;
}
