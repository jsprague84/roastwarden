import { describe, expect, it } from 'vitest';
import { smoothByTime } from './smooth.js';

const t = [0, 1, 2, 3, 4, 5];

describe('smoothByTime', () => {
	it('averages the window ending at each point (live)', () => {
		const out = smoothByTime(t, [0, 10, 0, 10, 0, 10], 2);
		// window (t−2, t]: 0 → [0]; 1 → [0,10]; 2 → [10,0]; …
		expect(out).toEqual([0, 5, 5, 5, 5, 5]);
	});

	it('averages around each point for a finished roast', () => {
		const out = smoothByTime(t, [0, 0, 30, 0, 0, 0], 2, true);
		// window (t−1, t+1]: t=1 → [0 (t=1), 30 (t=2)] = 15
		expect(out[1]).toBe(15);
		expect(out[2]).toBe(15);
		expect(out[4]).toBe(0);
	});

	it('keeps gaps and skips nulls inside a window', () => {
		const out = smoothByTime(t, [10, null, 20, 30, null, 40], 3);
		expect(out[1]).toBeNull();
		expect(out[2]).toBe(15);
		expect(out[3]).toBe(25);
		expect(out[5]).toBe(35);
	});

	it('tames a noisy heater', () => {
		const tt = Array.from({ length: 120 }, (_, i) => i);
		const noisy = tt.map((i) => (i % 2 ? 100 : 72));
		const out = smoothByTime(tt, noisy, 15).slice(20) as number[];
		expect(Math.max(...out) - Math.min(...out)).toBeLessThan(3);
	});

	it('passes values through with no window', () => {
		expect(smoothByTime([0, 1], [3, null], 0)).toEqual([3, null]);
	});
});
