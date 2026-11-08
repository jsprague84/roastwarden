import type { ProfileAction, ProfileEventInput } from '#lib/api/profiles.js';
import { eventLabels } from '#lib/constants/landmarks.js';

/** Landmark types a profile event may mark (custom is a note instead). */
export const LANDMARK_TYPES = [
	'turning_point',
	'drying_end',
	'first_crack_start',
	'first_crack_end',
	'second_crack_start',
	'second_crack_end',
	'development_start',
	'drop',
	'cool_end'
] as const;

export const ACTIONS: { value: ProfileAction; label: string }[] = [
	{ value: 'fan', label: 'Fan %' },
	{ value: 'heater', label: 'Heater %' },
	{ value: 'setpoint', label: 'Setpoint' },
	{ value: 'mode', label: 'Mode' },
	{ value: 'landmark', label: 'Landmark' },
	{ value: 'note', label: 'Note' }
];

export function blankEvent(atSeconds = 0): ProfileEventInput {
	return {
		trigger_kind: 'time',
		at_seconds: atSeconds,
		at_bt: null,
		action: 'fan',
		value: 50,
		label: null
	};
}

/** Actions that take a number in `value`; the others take text in `label`. */
export function isNumeric(action: ProfileAction): boolean {
	return action === 'fan' || action === 'heater' || action === 'setpoint';
}

/**
 * Short description, e.g. "Fan 60%". `temp` formats a °C value for display
 * (the setpoint is stored in °C).
 */
export function describeEvent(
	e: ProfileEventInput,
	temp: (c: number) => string = (c) => `${c.toFixed(0)}°C`
): string {
	switch (e.action) {
		case 'fan':
			return `Fan ${e.value ?? '?'}%`;
		case 'heater':
			return `Heater ${e.value ?? '?'}%`;
		case 'setpoint':
			return e.value == null ? 'SP ?' : `SP ${temp(e.value)}`;
		case 'mode':
			return e.label === 'manual' ? 'Manual' : 'Auto';
		case 'landmark':
			return eventLabels[e.label ?? ''] ?? e.label ?? 'Landmark';
		case 'note':
			return e.label ?? 'Note';
	}
}

/** Mirror of the server's checks, so the designer can flag problems before saving. */
export function validateEvent(e: ProfileEventInput): string | null {
	if (e.trigger_kind === 'time') {
		if (e.at_seconds == null || !Number.isFinite(e.at_seconds)) return 'needs a time';
		if (e.at_seconds < 0 || e.at_seconds > 7200) return 'time must be 0–7200 s';
	} else {
		if (e.at_bt == null || !Number.isFinite(e.at_bt)) return 'needs a BT';
		if (e.at_bt < 0 || e.at_bt > 350) return 'BT must be 0–350 °C';
	}
	const v = e.value;
	switch (e.action) {
		case 'fan':
		case 'heater':
			if (v == null || !Number.isFinite(v) || v < 0 || v > 100) return 'value must be 0–100 %';
			return null;
		case 'setpoint':
			if (v == null || !Number.isFinite(v) || v < 0 || v > 300) return 'setpoint must be 0–300 °C';
			return null;
		case 'mode':
			return e.label === 'auto' || e.label === 'manual' ? null : 'pick auto or manual';
		case 'landmark':
			return (LANDMARK_TYPES as readonly string[]).includes(e.label ?? '')
				? null
				: 'pick a landmark';
		case 'note':
			return e.label && e.label.trim() ? null : 'needs text';
	}
}

/** Strip an event to what the API accepts (drops id/profile_id/created_at). */
export function toInput(e: ProfileEventInput): ProfileEventInput {
	const numeric = isNumeric(e.action);
	return {
		trigger_kind: e.trigger_kind,
		at_seconds: e.at_seconds,
		at_bt: e.at_bt,
		action: e.action,
		value: numeric ? e.value : null,
		label: numeric ? e.label?.trim() || null : e.label
	};
}

/**
 * When an event fires on the curve, in seconds from CHARGE: its time, or for
 * a BT trigger the first time BT rises through `at_bt` after the curve's
 * minimum (the turning point). `curve` is `[t, bt]` pairs sorted by time.
 */
export function eventTimeOnCurve(e: ProfileEventInput, curve: [number, number][]): number | null {
	if (e.trigger_kind === 'time') return e.at_seconds;
	const target = e.at_bt;
	if (target == null || curve.length < 2) return null;
	let iMin = 0;
	for (let i = 1; i < curve.length; i++) if (curve[i][1] < curve[iMin][1]) iMin = i;
	for (let i = Math.max(1, iMin + 1); i < curve.length; i++) {
		const [t0, v0] = curve[i - 1];
		const [t1, v1] = curve[i];
		if (v0 <= target && v1 >= target) {
			return v1 === v0 ? t1 : t0 + ((target - v0) / (v1 - v0)) * (t1 - t0);
		}
	}
	return null;
}
