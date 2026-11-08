/**
 * Temperature display unit.
 *
 * The server, the firmware and every API payload are always Celsius. This
 * store only affects what the dashboard renders and what it parses from user
 * input: convert with `toDisplay` / `fromDisplay` at the edge, keep °C inside.
 *
 * The preference is per browser (localStorage), like Artisan's °C/°F mode.
 */

export type TempUnit = 'C' | 'F';

const STORAGE_KEY = 'roastwarden.temp_unit';

function loadUnit(): TempUnit {
	try {
		if (typeof localStorage !== 'undefined') {
			const v = localStorage.getItem(STORAGE_KEY);
			if (v === 'F' || v === 'C') return v;
		}
	} catch {
		// private mode / storage disabled: fall through to the default
	}
	return 'C';
}

export const unitState = $state<{ unit: TempUnit }>({ unit: loadUnit() });

export function setUnit(unit: TempUnit) {
	unitState.unit = unit;
	try {
		localStorage.setItem(STORAGE_KEY, unit);
	} catch {
		// ignore
	}
}

// --- Weight unit: per browser too (docs/lean-home.md, "Weight units") ---
//
// The server and every API payload are grams. Metric shows g, then kg from 1 kg; US shows oz,
// then lb from 1 lb, and a bare number typed in a weight field is grams or ounces.
// The pure formatting and parsing are in `lib/beans/weight.ts`.

export type WeightUnit = 'metric' | 'us';

const WEIGHT_KEY = 'roastwarden.weight_unit';

function loadWeightUnit(): WeightUnit {
	try {
		if (typeof localStorage !== 'undefined') {
			const v = localStorage.getItem(WEIGHT_KEY);
			if (v === 'metric' || v === 'us') return v;
		}
	} catch {
		// private mode / storage disabled: fall through to the default
	}
	return 'metric';
}

export const weightState = $state<{ unit: WeightUnit }>({ unit: loadWeightUnit() });

export function setWeightUnit(unit: WeightUnit) {
	weightState.unit = unit;
	try {
		localStorage.setItem(WEIGHT_KEY, unit);
	} catch {
		// ignore
	}
}

/** The unit a bare number in a weight field means: "g" or "oz". */
export function weightUnit(): 'g' | 'oz' {
	return weightState.unit === 'us' ? 'oz' : 'g';
}

// --- Pure conversions (unit passed explicitly; used by tests and helpers) ---

export function cToF(c: number): number {
	return c * 1.8 + 32;
}

export function fToC(f: number): number {
	return (f - 32) / 1.8;
}

/** Convert an absolute temperature in °C to the given unit. */
export function convertTemp(c: number, unit: TempUnit): number {
	return unit === 'F' ? cToF(c) : c;
}

/** Convert an absolute temperature in the given unit back to °C. */
export function convertTempToC(v: number, unit: TempUnit): number {
	return unit === 'F' ? fToC(v) : v;
}

/** Convert a temperature difference (delta, RoR, hysteresis) in °C to the unit. */
export function convertDelta(c: number, unit: TempUnit): number {
	return unit === 'F' ? c * 1.8 : c;
}

export function convertDeltaToC(v: number, unit: TempUnit): number {
	return unit === 'F' ? v / 1.8 : v;
}

// --- Reactive helpers (read `unitState.unit`, so they track the preference) ---

/** Absolute °C → display unit. */
export function toDisplay(c: number): number {
	return convertTemp(c, unitState.unit);
}

/** Display unit → absolute °C (for inputs). */
export function fromDisplay(v: number): number {
	return convertTempToC(v, unitState.unit);
}

/** Temperature difference °C → display unit. */
export function deltaToDisplay(c: number): number {
	return convertDelta(c, unitState.unit);
}

/** Temperature difference in display unit → °C. */
export function deltaFromDisplay(v: number): number {
	return convertDeltaToC(v, unitState.unit);
}

/** "°C" or "°F". */
export function tempUnit(): string {
	return unitState.unit === 'F' ? '°F' : '°C';
}

/** "°C/min" or "°F/min". */
export function rorUnit(): string {
	return `${tempUnit()}/min`;
}

/** Format an absolute °C value in the display unit, no suffix. `--` when missing. */
export function fmtTemp(c: number | null | undefined, decimals = 1): string {
	if (c == null || Number.isNaN(c)) return '--';
	return toDisplay(c).toFixed(decimals);
}

/** Format a °C difference (delta / RoR) in the display unit, no suffix. */
export function fmtDelta(c: number | null | undefined, decimals = 1, signed = false): string {
	if (c == null || Number.isNaN(c)) return '--';
	const v = deltaToDisplay(c);
	const sign = signed && v >= 0 ? '+' : '';
	return `${sign}${v.toFixed(decimals)}`;
}

/** Round a display-unit value to a sensible input step for the unit. */
export function roundForInput(v: number, decimals = 1): number {
	const p = 10 ** decimals;
	return Math.round(v * p) / p;
}
