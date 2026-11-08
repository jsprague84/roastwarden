import { describe, it, expect } from 'vitest';
import { fitMonotone, monotoneAt } from './monotone.js';

describe('monotone cubic', () => {
	const c = fitMonotone([
		[0, 30],
		[120, 120],
		[240, 160],
		[480, 200],
		[600, 212]
	])!;

	it('passes through the points and stays inside them', () => {
		for (const [x, y] of [
			[0, 30],
			[240, 160],
			[600, 212]
		])
			expect(monotoneAt(c, x)).toBeCloseTo(y, 9);
		let prev = -Infinity;
		for (let x = 0; x <= 600; x++) {
			const y = monotoneAt(c, x)!;
			expect(y).toBeGreaterThanOrEqual(prev);
			prev = y;
		}
		expect(monotoneAt(c, 601)).toBeNull();
	});

	it('has no corners at the points (the slope is continuous)', () => {
		const slope = (x: number) => (monotoneAt(c, x + 0.5)! - monotoneAt(c, x - 0.5)!) / 1;
		expect(Math.abs(slope(119.5) - slope(120.5))).toBeLessThan(0.02);
		expect(Math.abs(slope(239.5) - slope(240.5))).toBeLessThan(0.02);
	});

	it('flattens at a local minimum instead of overshooting', () => {
		const dip = fitMonotone([
			[0, 180],
			[60, 90],
			[120, 110]
		])!;
		for (let x = 0; x <= 120; x++) expect(monotoneAt(dip, x)!).toBeGreaterThanOrEqual(90 - 1e-9);
	});
});
