/**
 * Weights in the browser's system (docs/lean-home.md, "Weight units"): Metric (g, then kg from
 * 1 kg) or US (oz, then lb from 1 lb). Storage, the wire and the API are grams; these convert at
 * the edge. Pure: the unit is passed in (`lib/stores/units.svelte.ts` keeps the browser's).
 */

import type { WeightUnit } from '#lib/stores/units.svelte.js';

const G_PER_OZ = 28.349523125;
const G_PER_LB = 453.59237;

const num = (v: number, max: number) => v.toLocaleString(undefined, { maximumFractionDigits: max });

/**
 * "850 g", "1.25 kg"; "8.8 oz", "0.63 oz", "2.21 lb"; "−150 g". "–" when missing. `precise`
 * for a roast's weights on a scale: grams to 0.01 ("85.75 g"), ounces to 0.01 ("3.02 oz").
 */
export function formatWeight(
	g: number | null | undefined,
	unit: WeightUnit,
	{ precise = false }: { precise?: boolean } = {}
): string {
	if (g == null || !Number.isFinite(g)) return '–';
	const sign = g < 0 ? '−' : '';
	const a = Math.abs(g);
	if (unit === 'us') {
		const oz = a / G_PER_OZ;
		// 1 decimal (2 under 1 oz, a dose); from 16.0 oz as shown, pounds
		if (Math.round(oz * 10) / 10 >= 16)
			return `${sign}${num(Math.round((a / G_PER_LB) * 100) / 100, 2)} lb`;
		const d = precise || oz < 1 ? 2 : 1;
		return `${sign}${num(Math.round(oz * 10 ** d) / 10 ** d, d)} oz`;
	}
	if (a >= 1000) return `${sign}${num(Math.round(a / 10) / 100, 2)} kg`;
	return precise ? `${sign}${num(Math.round(a * 100) / 100, 2)} g` : `${sign}${Math.round(a)} g`;
}

/** The unit a bare number means in a weight field: "g" or "oz". */
export function smallUnit(unit: WeightUnit): 'g' | 'oz' {
	return unit === 'us' ? 'oz' : 'g';
}

/** The unit for a price per weight: "kg" or "lb". */
export function bigUnit(unit: WeightUnit): 'kg' | 'lb' {
	return unit === 'us' ? 'lb' : 'kg';
}

/** A price per kg as a price per the system's big unit (kg or lb). */
export function perBigUnit(perKg: number, unit: WeightUnit): number {
	return unit === 'us' ? (perKg * G_PER_LB) / 1000 : perKg;
}

const FACTOR: Record<string, number> = {
	g: 1,
	gr: 1,
	gram: 1,
	grams: 1,
	kg: 1000,
	kgs: 1000,
	kilo: 1000,
	kilos: 1000,
	lb: G_PER_LB,
	lbs: G_PER_LB,
	pound: G_PER_LB,
	pounds: G_PER_LB,
	oz: G_PER_OZ,
	ounce: G_PER_OZ,
	ounces: G_PER_OZ
};

/**
 * A typed weight to grams: "250 g", "250g", "8 oz", "2 lb", "1.5kg", "0,5 lb" (the CLI's forms);
 * a bare number is the system's small unit (g or oz). Grams as typed (a scale reads 85.75 g),
 * a converted unit to 0.1 g, as the CLI does. Negative only with `signed` (a correction).
 * An error message when it is not a weight.
 */
export function parseWeight(
	text: string,
	unit: WeightUnit,
	{ signed = false }: { signed?: boolean } = {}
): number | string {
	const t = text.trim().toLowerCase().replace(/\s+/g, '').replace('−', '-');
	const eg = unit === 'us' ? '8 oz, 2 lb' : '250 g, 1.5 kg';
	if (!t) return `Type a weight (${eg})`;
	const m = /^([+-]?)(\d+(?:[.,]\d*)?|[.,]\d+)([a-z]*)$/.exec(t);
	if (!m) return `Not a weight: "${text.trim()}" (${eg})`;
	if (m[1] === '-' && !signed) return 'A weight is not below 0';
	const n = Number(m[2].replace(',', '.')) * (m[1] === '-' ? -1 : 1);
	const factor = m[3] === '' ? FACTOR[smallUnit(unit)] : FACTOR[m[3]];
	if (factor == null) return `Unknown unit "${m[3]}" (g, kg, oz or lb)`;
	const g = n * factor;
	if (!Number.isFinite(g)) return `Not a weight: "${text.trim()}"`;
	return factor === 1 ? g : Math.round(g * 10) / 10 || 0;
}

/**
 * What a weight field shows for a stored weight: grams as stored in Metric ("100", "85.75"), in
 * US ounces or pounds with the unit ("3.53 oz", "2.2 lb"). The field keeps the stored grams until
 * it is edited, so showing a rounded value never changes what is saved.
 */
export function weightInputText(g: number | null | undefined, unit: WeightUnit): string {
	if (g == null || !Number.isFinite(g)) return '';
	if (unit === 'metric') return String(Math.round(g * 100) / 100);
	const oz = Math.abs(g) / G_PER_OZ;
	if (Math.round(oz * 100) / 100 >= 16) return `${Math.round((g / G_PER_LB) * 1000) / 1000} lb`;
	return `${Math.round((g / G_PER_OZ) * 100) / 100} oz`;
}
