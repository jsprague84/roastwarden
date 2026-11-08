import { describe, expect, it } from 'vitest';
import { labelRows, labelWidth } from './labels.js';

describe('labelRows', () => {
	it('keeps labels that are far apart on one row', () => {
		expect(labelRows([0, 100, 200], [40, 40, 40], 1)).toEqual([0, 0, 0]);
	});

	it('stacks labels that would overlap', () => {
		// FC Start, FC End and Drop within a few pixels of each other on a phone
		expect(labelRows([470, 500, 520], [50, 44, 30], 0.5)).toEqual([0, 1, 2]);
	});

	it('reuses a row once it is clear again', () => {
		expect(labelRows([0, 10, 100], [40, 40, 40], 1)).toEqual([0, 1, 0]);
	});

	it('answers in input order whatever the order of the positions', () => {
		expect(labelRows([10, 0], [40, 40], 1)).toEqual([1, 0]);
	});

	it('measures longer text as wider', () => {
		expect(labelWidth('FC Start')).toBeGreaterThan(labelWidth('Drop'));
	});
});
