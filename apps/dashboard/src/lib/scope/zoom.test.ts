import { describe, expect, it } from 'vitest';
import { clampRange, panWindow, pinchWindow, zoomWindow } from './zoom.js';

const xs = Array.from({ length: 601 }, (_, i) => i); // 0..600 s

describe('the x window', () => {
	it('clamps to the data, keeping the span', () => {
		expect(clampRange(xs, 100, 200)).toEqual([100, 200]);
		expect(clampRange(xs, -50, 50)).toEqual([0, 100]);
		expect(clampRange(xs, 550, 650)).toEqual([500, 600]);
	});

	it('is at least 10 s wide and never wider than the data', () => {
		expect(clampRange(xs, 100, 102)).toEqual([100, 110]);
		expect(clampRange(xs, -100, 1000)).toEqual([0, 600]);
		expect(clampRange([], 5, 7)).toEqual([-3, 7]); // no data: the span is 10 s, pinned to the end asked
	});
});

describe('a wheel zoom', () => {
	it('shrinks the window about the pointer and says when it shows everything', () => {
		const zoomIn = zoomWindow(xs, [0, 600], 300, 0.5);
		expect(zoomIn.range).toEqual([150, 450]);
		expect(zoomIn.full).toBe(false);
		const zoomOut = zoomWindow(xs, [150, 450], 300, 2);
		expect(zoomOut.range).toEqual([0, 600]);
		expect(zoomOut.full).toBe(true);
	});

	it('keeps the pointer at the same place in the window', () => {
		const z = zoomWindow(xs, [0, 600], 50, 0.5);
		expect(z.range).toEqual([25, 325]);
		expect(z.full).toBe(false);
		// a zoom out near the start pins the window to the data's start
		expect(zoomWindow(xs, [25, 325], 50, 2).range).toEqual([0, 600]);
	});
});

describe('touch', () => {
	it('pinches about the centre and pans by seconds, clamped', () => {
		expect(pinchWindow(xs, { min: 100, max: 300, center: 200 }, 0.5)).toEqual([150, 250]);
		expect(pinchWindow(xs, { min: 100, max: 300, center: 200 }, 4)).toEqual([0, 600]);
		expect(panWindow(xs, { min: 100, max: 300 }, 50)).toEqual([150, 350]);
		expect(panWindow(xs, { min: 100, max: 300 }, -200)).toEqual([0, 200]);
	});
});
