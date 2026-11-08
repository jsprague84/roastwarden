import { describe, it, expect } from 'vitest';
import {
	parseFlags,
	joinFlags,
	colorText,
	numOrNull,
	numOrUndefined,
	purposeLabel
} from './properties.js';

describe('roast properties', () => {
	it('round-trips flags', () => {
		expect(parseFlags(' tipping, scorching ,')).toEqual(['tipping', 'scorching']);
		expect(joinFlags(['tipping', 'tipping', ' '])).toBe('tipping');
		expect(joinFlags([])).toBeNull();
		expect(parseFlags(null)).toEqual([]);
	});
	it('formats colour and numbers', () => {
		expect(colorText(68, 74, 'agtron')).toBe('Agtron 68 / 74');
		expect(colorText(null, 74, null)).toBe('Colour – / 74');
		expect(colorText(null, null, 'agtron')).toBeNull();
		expect(numOrNull('')).toBeNull();
		expect(numOrNull('11.5')).toBe(11.5);
		expect(numOrNull('x')).toBeNull();
		expect(purposeLabel('espresso')).toBe('Espresso');
	});

	// A Svelte 5 number input binds a number once typed (the roasted weight failed to save
	// with "v.trim is not a function") and null once cleared.
	it('reads what number inputs bind', () => {
		expect(numOrNull(86.4)).toBe(86.4);
		expect(numOrNull(0)).toBe(0);
		expect(numOrUndefined(86.4)).toBe(86.4);
		expect(numOrUndefined('100')).toBe(100);
		expect(numOrUndefined(null)).toBeUndefined();
		expect(numOrUndefined('')).toBeUndefined();
	});
});
