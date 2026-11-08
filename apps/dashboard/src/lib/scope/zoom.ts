/**
 * The scope's zoom and pan maths: the x window clamped to the data, a wheel zoom about a
 * point, a pinch and a touch pan. Times are scope seconds; `xs` is the x series (the live
 * samples and the future drawn ahead of them).
 */

/** The window clamped to the data: at least 10 s wide, never past either end. */
export function clampRange(xs: number[], min: number, max: number): [number, number] {
	const dmin = xs[0] ?? min;
	const dmax = xs[xs.length - 1] ?? max;
	const span = Math.max(10, Math.min(max - min, dmax - dmin));
	let a = Math.max(dmin, min);
	let b = a + span;
	if (b > dmax) {
		b = dmax;
		a = b - span;
	}
	return [a, b];
}

/** The window after a wheel zoom about `xVal` by `factor` (below 1 zooms in); `full` when it
 * shows all the data again. */
export function zoomWindow(
	xs: number[],
	current: [number, number],
	xVal: number,
	factor: number
): { range: [number, number]; full: boolean } {
	const [min, max] = current;
	const left = (xVal - min) * factor;
	const right = (max - xVal) * factor;
	const range = clampRange(xs, xVal - left, xVal + right);
	const full = range[0] <= (xs[0] ?? range[0]) && range[1] >= (xs[xs.length - 1] ?? range[1]);
	return { range, full };
}

/** The window after a pinch: the window at its start scaled by `factor` about its centre. */
export function pinchWindow(
	xs: number[],
	start: { min: number; max: number; center: number },
	factor: number
): [number, number] {
	const c = start.center;
	return clampRange(xs, c - (c - start.min) * factor, c + (start.max - c) * factor);
}

/** The window at a pan's start moved by `dv` seconds. */
export function panWindow(
	xs: number[],
	start: { min: number; max: number },
	dv: number
): [number, number] {
	return clampRange(xs, start.min + dv, start.max + dv);
}
