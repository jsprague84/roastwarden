import { describe, it, expect } from 'vitest';
import {
	cToF,
	fToC,
	convertTemp,
	convertTempToC,
	convertDelta,
	convertDeltaToC
} from './units.svelte.js';

describe('temperature unit conversions', () => {
	it('converts absolute temperatures', () => {
		expect(cToF(0)).toBeCloseTo(32);
		expect(cToF(100)).toBeCloseTo(212);
		expect(cToF(200)).toBeCloseTo(392);
		expect(fToC(392)).toBeCloseTo(200);
		expect(convertTemp(185.5, 'C')).toBe(185.5);
		expect(convertTemp(185.5, 'F')).toBeCloseTo(365.9);
		expect(convertTempToC(convertTemp(173.2, 'F'), 'F')).toBeCloseTo(173.2);
	});

	it('converts differences without the 32° offset', () => {
		expect(convertDelta(10, 'F')).toBeCloseTo(18);
		expect(convertDelta(10, 'C')).toBe(10);
		expect(convertDeltaToC(18, 'F')).toBeCloseTo(10);
		expect(convertDelta(-2.5, 'F')).toBeCloseTo(-4.5);
	});
});
