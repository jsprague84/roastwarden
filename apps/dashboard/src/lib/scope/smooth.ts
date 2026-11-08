/**
 * A display-only moving average by time, for the heater line (Settings → This browser,
 * "Heater on charts"). The PID's output swings by about 14 points a second on the fluid bed,
 * which hides what the heater is doing; an average over 15 s shows the trend.
 *
 * - `centered: false` (the live scope): each point averages the window ending at it, so it
 *   needs no future samples and lags by about half the window.
 * - `centered: true` (a finished roast): each point averages the window around it, no lag.
 *
 * Gaps (null) are skipped, and a point with no sample in its window stays null. Linear in
 * the number of points.
 */
export function smoothByTime(
	t: readonly number[],
	v: readonly (number | null | undefined)[],
	windowSec: number,
	centered = false
): (number | null)[] {
	const n = Math.min(t.length, v.length);
	const out: (number | null)[] = new Array(n).fill(null);
	if (windowSec <= 0) {
		for (let i = 0; i < n; i++) out[i] = v[i] ?? null;
		return out;
	}
	const before = centered ? windowSec / 2 : windowSec;
	const after = centered ? windowSec / 2 : 0;
	let lo = 0;
	let hi = 0; // window is [lo, hi)
	let sum = 0;
	let count = 0;
	const add = (k: number) => {
		const x = v[k];
		if (x != null && Number.isFinite(x)) {
			sum += x;
			count++;
		}
	};
	const drop = (k: number) => {
		const x = v[k];
		if (x != null && Number.isFinite(x)) {
			sum -= x;
			count--;
		}
	};
	for (let i = 0; i < n; i++) {
		while (hi < n && t[hi] <= t[i] + after) add(hi++);
		while (lo < hi && t[lo] <= t[i] - before) drop(lo++);
		// A gap in the data stays a gap rather than a stretched average
		out[i] = v[i] == null || count === 0 ? null : sum / count;
	}
	return out;
}

/** The window the heater line is averaged over when smoothing is on. */
export const HEATER_SMOOTH_SEC = 15;
