import { describe, it, expect } from 'vitest';
import {
	fitNaturalCubicSpline,
	evaluateSpline,
	evaluateSplineDerivative,
	evaluateSplineGrid,
	evaluateDerivativeGrid,
	type SplinePoint
} from './spline.js';

describe('fitNaturalCubicSpline', () => {
	it('returns empty coefficients for 0 points', () => {
		const spline = fitNaturalCubicSpline([]);
		expect(spline.xs).toEqual([]);
		expect(evaluateSpline(spline, 5)).toBe(0);
	});

	it('returns constant for 1 point', () => {
		const spline = fitNaturalCubicSpline([{ x: 10, y: 42 }]);
		expect(evaluateSpline(spline, 0)).toBe(42);
		expect(evaluateSpline(spline, 10)).toBe(42);
		expect(evaluateSpline(spline, 100)).toBe(42);
		expect(evaluateSplineDerivative(spline, 10)).toBe(0);
	});

	it('produces linear interpolation for 2 points', () => {
		const spline = fitNaturalCubicSpline([
			{ x: 0, y: 0 },
			{ x: 10, y: 20 }
		]);

		// Passes through endpoints
		expect(evaluateSpline(spline, 0)).toBeCloseTo(0, 10);
		expect(evaluateSpline(spline, 10)).toBeCloseTo(20, 10);

		// Midpoint is linear
		expect(evaluateSpline(spline, 5)).toBeCloseTo(10, 10);

		// Derivative is constant slope = 2
		expect(evaluateSplineDerivative(spline, 3)).toBeCloseTo(2, 10);
		expect(evaluateSplineDerivative(spline, 7)).toBeCloseTo(2, 10);
	});

	it('passes through all knot points', () => {
		const points: SplinePoint[] = [
			{ x: 0, y: 25 },
			{ x: 60, y: 100 },
			{ x: 180, y: 170 },
			{ x: 360, y: 200 },
			{ x: 600, y: 215 }
		];
		const spline = fitNaturalCubicSpline(points);

		for (const p of points) {
			expect(evaluateSpline(spline, p.x)).toBeCloseTo(p.y, 8);
		}
	});

	it('closely approximates a smooth curve between many knots', () => {
		// Natural cubic spline with many knots should closely match the original function
		// f(x) = 2 + 3x - x²  sampled at 5 points
		const f = (x: number) => 2 + 3 * x - x * x;
		const fPrime = (x: number) => 3 - 2 * x;

		const points: SplinePoint[] = [
			{ x: 0, y: f(0) },
			{ x: 0.5, y: f(0.5) },
			{ x: 1, y: f(1) },
			{ x: 1.5, y: f(1.5) },
			{ x: 2, y: f(2) }
		];
		const spline = fitNaturalCubicSpline(points);

		// Test at intermediate points — with 5 knots, errors should be small
		for (const x of [0.25, 0.75, 1.25, 1.75]) {
			expect(evaluateSpline(spline, x)).toBeCloseTo(f(x), 1);
			expect(evaluateSplineDerivative(spline, x)).toBeCloseTo(fPrime(x), 0);
		}
	});

	it('clamps to endpoints outside range', () => {
		const spline = fitNaturalCubicSpline([
			{ x: 0, y: 50 },
			{ x: 100, y: 200 },
			{ x: 300, y: 210 }
		]);

		// Before start → first y
		expect(evaluateSpline(spline, -10)).toBe(50);
		// After end → last y
		expect(evaluateSpline(spline, 500)).toBe(210);

		// Derivative outside range → 0
		expect(evaluateSplineDerivative(spline, -5)).toBe(0);
		expect(evaluateSplineDerivative(spline, 400)).toBe(0);
	});

	it('produces smooth RoR for a realistic roast profile', () => {
		// Typical roast: charge → drying → Maillard → development
		const points: SplinePoint[] = [
			{ x: 0, y: 25 }, // charge
			{ x: 60, y: 100 }, // ramp up
			{ x: 180, y: 150 }, // drying end
			{ x: 300, y: 190 }, // Maillard
			{ x: 420, y: 200 }, // FC start
			{ x: 540, y: 210 }, // development
			{ x: 660, y: 215 } // drop
		];
		const spline = fitNaturalCubicSpline(points);

		// Evaluate RoR at 5s intervals and check no wild oscillations
		const rorValues: number[] = [];
		for (let t = 5; t <= 655; t += 5) {
			const ror = evaluateSplineDerivative(spline, t) * 60; // °C/min
			rorValues.push(ror);
		}

		// All RoR values should be positive (monotonically increasing profile)
		// Allow some tolerance near endpoints where natural BC might cause slight dips
		const interiorRor = rorValues.slice(2, -2);
		for (const ror of interiorRor) {
			expect(ror).toBeGreaterThan(-5); // no extreme negative spikes
			expect(ror).toBeLessThan(200); // no extreme positive spikes
		}

		// RoR should generally decrease (declining RoR is typical)
		// Check that RoR at end is less than at start (within interior)
		const earlyRor = rorValues[10]; // ~50s mark
		const lateRor = rorValues[rorValues.length - 10];
		expect(lateRor).toBeLessThan(earlyRor);
	});

	it('deduplicates points with the same x', () => {
		const spline = fitNaturalCubicSpline([
			{ x: 0, y: 10 },
			{ x: 50, y: 100 },
			{ x: 50, y: 120 }, // duplicate x, should keep last (120)
			{ x: 100, y: 200 }
		]);

		expect(evaluateSpline(spline, 50)).toBeCloseTo(120, 8);
		expect(spline.xs).toHaveLength(3);
	});
});

describe('evaluateSplineGrid', () => {
	it('matches individual evaluations', () => {
		const spline = fitNaturalCubicSpline([
			{ x: 0, y: 50 },
			{ x: 100, y: 150 },
			{ x: 200, y: 180 },
			{ x: 300, y: 210 }
		]);

		const grid = evaluateSplineGrid(spline, 0, 300, 10);

		for (const [x, y] of grid) {
			expect(y).toBeCloseTo(evaluateSpline(spline, x), 10);
		}

		// Should have 31 points (0, 10, 20, ..., 300)
		expect(grid).toHaveLength(31);
	});
});

describe('evaluateDerivativeGrid', () => {
	it('matches individual derivative evaluations', () => {
		const spline = fitNaturalCubicSpline([
			{ x: 0, y: 50 },
			{ x: 100, y: 150 },
			{ x: 200, y: 180 },
			{ x: 300, y: 210 }
		]);

		const grid = evaluateDerivativeGrid(spline, 0, 300, 15);

		for (const [x, dy] of grid) {
			expect(dy).toBeCloseTo(evaluateSplineDerivative(spline, x), 10);
		}
	});

	it('returns empty for empty spline', () => {
		const spline = fitNaturalCubicSpline([]);
		expect(evaluateDerivativeGrid(spline, 0, 100, 10)).toEqual([]);
	});
});
