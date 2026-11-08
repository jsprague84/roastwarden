import { describe, expect, it } from 'vitest';
import { indexAt, rorRange, tempRange } from './frame.js';

describe('indexAt', () => {
	const t = [-3, -2, 0, 1, 2.5, 4];
	it('finds the last sample at or before a time', () => {
		expect(indexAt(t, -10)).toBe(0);
		expect(indexAt(t, 0)).toBe(2);
		expect(indexAt(t, 2.4)).toBe(3);
		expect(indexAt(t, 2.5)).toBe(4);
		expect(indexAt(t, 99)).toBe(5);
	});
});

describe('tempRange', () => {
	it('pads to round tens and keeps a 50° span', () => {
		expect(tempRange([24.4, null, 209.8])).toEqual([10, 220]);
		expect(tempRange([20, 21])).toEqual([10, 60]);
	});
});

describe('rorRange', () => {
	const c = (v: number) => v;
	it('clips the charge drop and caps the top as the app does', () => {
		expect(rorRange([-200, 64.6, 10], c)).toEqual([-30, 60]);
		expect(rorRange([2, 18], c)).toEqual([-5, 25]);
	});
	it('works in °F deltas', () => {
		const f = (v: number) => (v * 9) / 5;
		expect(rorRange([-200, 116], f)).toEqual([-54, 110]);
	});
});
