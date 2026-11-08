/**
 * Monotone cubic interpolation (Fritsch–Carlson PCHIP), the same curve the
 * server's Artisan export draws through a profile's points: smooth, so a
 * rate of rise taken from it has no steps at the points, and never above or
 * below the points it runs between.
 */
export interface Monotone {
	xs: number[];
	ys: number[];
	ms: number[];
}

export function fitMonotone(points: [number, number][]): Monotone | null {
	const byX = new Map<number, number>();
	for (const [x, y] of points) if (Number.isFinite(x) && Number.isFinite(y)) byX.set(x, y);
	const pts = [...byX.entries()].sort((a, b) => a[0] - b[0]);
	const n = pts.length;
	if (n < 2) return null;
	const xs = pts.map((p) => p[0]);
	const ys = pts.map((p) => p[1]);
	const h = xs.slice(1).map((x, i) => x - xs[i]);
	const d = ys.slice(1).map((y, i) => (y - ys[i]) / h[i]);
	const ms = new Array<number>(n).fill(0);
	if (n === 2) {
		ms[0] = ms[1] = d[0];
	} else {
		for (let i = 1; i < n - 1; i++) {
			if (d[i - 1] * d[i] > 0) {
				const w1 = 2 * h[i] + h[i - 1];
				const w2 = h[i] + 2 * h[i - 1];
				ms[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]);
			}
		}
		ms[0] = endSlope(h[0], h[1], d[0], d[1]);
		ms[n - 1] = endSlope(h[n - 2], h[n - 3], d[n - 2], d[n - 3]);
	}
	return { xs, ys, ms };
}

function endSlope(h0: number, h1: number, d0: number, d1: number): number {
	const m = ((2 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
	if (m * d0 <= 0) return 0;
	if (d0 * d1 <= 0 && Math.abs(m) > Math.abs(3 * d0)) return 3 * d0;
	return m;
}

/** The curve at `x`, or null outside its points. */
export function monotoneAt(c: Monotone, x: number): number | null {
	const n = c.xs.length;
	if (x < c.xs[0] - 1e-9 || x > c.xs[n - 1] + 1e-9) return null;
	let i = 0;
	let hi = n - 1;
	while (hi - i > 1) {
		const mid = (i + hi) >> 1;
		if (c.xs[mid] <= x) i = mid;
		else hi = mid;
	}
	const h = c.xs[i + 1] - c.xs[i];
	const s = (x - c.xs[i]) / h;
	const s2 = s * s;
	const s3 = s2 * s;
	return (
		(2 * s3 - 3 * s2 + 1) * c.ys[i] +
		(s3 - 2 * s2 + s) * h * c.ms[i] +
		(-2 * s3 + 3 * s2) * c.ys[i + 1] +
		(s3 - s2) * h * c.ms[i + 1]
	);
}
