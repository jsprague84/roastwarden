/** The roast demo's arithmetic (main.ts), kept apart so it is tested without a page. */
import type { Num } from '#lib/scope/scopeData.js';

/** The index of the last sample at or before `sec`. */
export function indexAt(t: number[], sec: number): number {
	let lo = 0;
	let hi = t.length - 1;
	if (sec <= t[0]) return 0;
	while (lo < hi) {
		const mid = (lo + hi + 1) >> 1;
		if (t[mid] <= sec) lo = mid;
		else hi = mid - 1;
	}
	return lo;
}

/** The temperature axis for the whole roast: round tens with 10° to spare, as the app's. */
export function tempRange(values: Num[]): [number, number] {
	let min = Infinity;
	let max = -Infinity;
	for (const v of values) {
		if (v == null) continue;
		min = Math.min(min, v);
		max = Math.max(max, v);
	}
	const lo = Math.floor((min - 10) / 10) * 10;
	const hi = Math.ceil((max + 10) / 10) * 10;
	return [lo, Math.max(hi, lo + 50)];
}

/** The RoR axis as the app sets it, in display units (`d` converts a °C delta). */
export function rorRange(values: Num[], d: (c: number) => number): [number, number] {
	let min = Infinity;
	let max = -Infinity;
	for (const v of values) {
		if (v == null) continue;
		min = Math.min(min, v);
		max = Math.max(max, v);
	}
	const lo = Math.max(d(-30), Math.min(d(-5), Math.floor(Math.min(min, 0) / 5) * 5));
	const hi = Math.max(d(25), Math.ceil(Math.min(Math.max(max, 0), d(60)) / 5) * 5);
	return [lo, hi];
}
