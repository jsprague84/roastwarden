import { describe, expect, it } from 'vitest';
import { axisLayout, clockStep, COMPACT_SCOPE_PX, TIME_INCRS, yAxesWidth } from './axes.js';

describe('axisLayout', () => {
	it('keeps the full axes on a laptop', () => {
		const l = axisLayout(1000);
		expect(l.compact).toBe(false);
		expect(l.labels).toBe(true);
		expect(yAxesWidth(l)).toBe(148);
	});
	it('frees room on a phone: narrower axes, no labels, no % axis', () => {
		const l = axisLayout(360);
		expect(l.compact).toBe(true);
		expect(l.pct).toBeNull();
		expect(yAxesWidth(l)).toBeLessThanOrEqual(66);
		// a 360 px chart plots at least 290 px instead of about 210
		expect(360 - yAxesWidth(l)).toBeGreaterThanOrEqual(290);
	});
	it('switches at the threshold', () => {
		expect(axisLayout(COMPACT_SCOPE_PX - 1).compact).toBe(true);
		expect(axisLayout(COMPACT_SCOPE_PX).compact).toBe(false);
	});
});

describe('clock steps', () => {
	it('steps a 10 min roast in whole minutes, never 1:40', () => {
		expect(clockStep(600, 6)).toBe(120);
		expect(clockStep(600, 10)).toBe(60);
		expect(clockStep(605, 5)).toBe(180);
		expect(TIME_INCRS).not.toContain(100);
		expect(TIME_INCRS).not.toContain(200);
	});
	it('falls back to the largest step for a very long span', () => {
		expect(clockStep(100_000, 3)).toBe(7200);
		expect(clockStep(30, 0)).toBe(30);
	});
});
