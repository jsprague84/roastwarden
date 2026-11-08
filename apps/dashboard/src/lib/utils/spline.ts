export interface SplinePoint {
	x: number;
	y: number;
}

export interface SplineCoefficients {
	xs: number[];
	a: number[];
	b: number[];
	c: number[];
	d: number[];
}

/**
 * Fit a natural cubic spline through the given sorted points.
 *
 * Natural boundary conditions: S''(x_0) = S''(x_n) = 0  (i.e. c_0 = c_n = 0).
 * Tridiagonal system solved via Thomas algorithm — O(n).
 *
 * Each segment i uses:
 *   S_i(x) = a_i + b_i·dx + c_i·dx² + d_i·dx³
 * where dx = x - x_i.
 */
export function fitNaturalCubicSpline(points: SplinePoint[]): SplineCoefficients {
	// Deduplicate by x (keep last occurrence)
	const deduped = new Map<number, number>();
	for (const p of points) {
		deduped.set(p.x, p.y);
	}
	const sorted = [...deduped.entries()].sort((a, b) => a[0] - b[0]).map(([x, y]) => ({ x, y }));

	const n = sorted.length;

	if (n === 0) {
		return { xs: [], a: [], b: [], c: [], d: [] };
	}
	if (n === 1) {
		return { xs: [sorted[0].x], a: [sorted[0].y], b: [0], c: [0], d: [0] };
	}
	if (n === 2) {
		// Linear fallback
		const dx = sorted[1].x - sorted[0].x;
		const slope = dx !== 0 ? (sorted[1].y - sorted[0].y) / dx : 0;
		return {
			xs: [sorted[0].x, sorted[1].x],
			a: [sorted[0].y, sorted[1].y],
			b: [slope, slope],
			c: [0, 0],
			d: [0, 0]
		};
	}

	// n >= 3 — full cubic spline
	const xs = sorted.map((p) => p.x);
	const ys = sorted.map((p) => p.y);
	const segments = n - 1;

	// h[i] = x[i+1] - x[i]
	const h = new Float64Array(segments);
	for (let i = 0; i < segments; i++) {
		h[i] = xs[i + 1] - xs[i];
	}

	// Set up tridiagonal system for c coefficients (natural BCs → c[0]=0, c[n-1]=0)
	// Interior equations (i = 1..n-2):
	//   h[i-1]·c[i-1] + 2(h[i-1]+h[i])·c[i] + h[i]·c[i+1]
	//     = 3·((y[i+1]-y[i])/h[i] - (y[i]-y[i-1])/h[i-1])
	const interior = n - 2;
	const diag = new Float64Array(interior); // main diagonal
	const upper = new Float64Array(interior); // upper diagonal
	const lower = new Float64Array(interior); // lower diagonal
	const rhs = new Float64Array(interior);

	for (let i = 0; i < interior; i++) {
		const j = i + 1; // index into xs/ys
		diag[i] = 2 * (h[j - 1] + h[j]);
		if (i > 0) lower[i] = h[j - 1];
		if (i < interior - 1) upper[i] = h[j];
		rhs[i] = 3 * ((ys[j + 1] - ys[j]) / h[j] - (ys[j] - ys[j - 1]) / h[j - 1]);
	}

	// Thomas algorithm (forward sweep)
	for (let i = 1; i < interior; i++) {
		const m = lower[i] / diag[i - 1];
		diag[i] -= m * upper[i - 1];
		rhs[i] -= m * rhs[i - 1];
	}

	// Back substitution
	const cInterior = new Float64Array(interior);
	cInterior[interior - 1] = rhs[interior - 1] / diag[interior - 1];
	for (let i = interior - 2; i >= 0; i--) {
		cInterior[i] = (rhs[i] - upper[i] * cInterior[i + 1]) / diag[i];
	}

	// Full c array (c[0] = 0, c[n-1] = 0 for natural spline)
	const c = new Float64Array(n);
	for (let i = 0; i < interior; i++) {
		c[i + 1] = cInterior[i];
	}

	// Compute b and d from c
	const a = new Float64Array(n);
	const b = new Float64Array(n);
	const d = new Float64Array(n);

	for (let i = 0; i < n; i++) {
		a[i] = ys[i];
	}

	for (let i = 0; i < segments; i++) {
		b[i] = (ys[i + 1] - ys[i]) / h[i] - (h[i] * (2 * c[i] + c[i + 1])) / 3;
		d[i] = (c[i + 1] - c[i]) / (3 * h[i]);
	}
	// Last segment coefficients (unused for evaluation but kept for consistency)
	b[segments] =
		b[segments - 1] +
		2 * c[segments - 1] * h[segments - 1] +
		3 * d[segments - 1] * h[segments - 1] * h[segments - 1];

	return {
		xs: [...xs],
		a: [...a],
		b: [...b],
		c: [...c],
		d: [...d]
	};
}

/** Find the segment index for x (binary search). Clamps to valid range. */
function findSegment(xs: number[], x: number): number {
	const n = xs.length;
	if (n <= 1) return 0;
	if (x <= xs[0]) return 0;
	if (x >= xs[n - 1]) return n - 2;

	let lo = 0;
	let hi = n - 2;
	while (lo < hi) {
		const mid = (lo + hi) >> 1;
		if (xs[mid + 1] <= x) {
			lo = mid + 1;
		} else {
			hi = mid;
		}
	}
	return lo;
}

/** Evaluate the spline at x. Clamps to endpoint values outside the range. */
export function evaluateSpline(spline: SplineCoefficients, x: number): number {
	const { xs, a, b, c, d } = spline;
	if (xs.length === 0) return 0;
	if (xs.length === 1) return a[0];

	// Clamp to endpoints
	if (x <= xs[0]) return a[0];
	if (x >= xs[xs.length - 1]) return a[xs.length - 1];

	const i = findSegment(xs, x);
	const dx = x - xs[i];
	return a[i] + b[i] * dx + c[i] * dx * dx + d[i] * dx * dx * dx;
}

/** Evaluate the first derivative S'(x) = b + 2c·dx + 3d·dx². Returns 0 outside range. */
export function evaluateSplineDerivative(spline: SplineCoefficients, x: number): number {
	const { xs, b, c, d } = spline;
	if (xs.length <= 1) return 0;
	if (x < xs[0] || x > xs[xs.length - 1]) return 0;

	const i = findSegment(xs, x);
	const dx = x - xs[i];
	return b[i] + 2 * c[i] * dx + 3 * d[i] * dx * dx;
}

/** Evaluate the spline on a regular grid [xMin, xMax] with the given step. */
export function evaluateSplineGrid(
	spline: SplineCoefficients,
	xMin: number,
	xMax: number,
	step: number
): [number, number][] {
	if (spline.xs.length === 0) return [];
	const result: [number, number][] = [];
	for (let x = xMin; x <= xMax; x += step) {
		result.push([x, evaluateSpline(spline, x)]);
	}
	return result;
}

/** Evaluate the derivative on a regular grid [xMin, xMax] with the given step. */
export function evaluateDerivativeGrid(
	spline: SplineCoefficients,
	xMin: number,
	xMax: number,
	step: number
): [number, number][] {
	if (spline.xs.length === 0) return [];
	const result: [number, number][] = [];
	for (let x = xMin; x <= xMax; x += step) {
		result.push([x, evaluateSplineDerivative(spline, x)]);
	}
	return result;
}
