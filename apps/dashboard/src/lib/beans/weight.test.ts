import { describe, expect, it } from 'vitest';
import { formatWeight, parseWeight, perBigUnit, smallUnit, weightInputText } from './weight.js';

describe('weights in the browser system', () => {
	it('formats metric as before: g, then kg from 1 kg', () => {
		expect(formatWeight(850, 'metric')).toBe('850 g');
		expect(formatWeight(1250, 'metric')).toBe('1.25 kg');
		expect(formatWeight(-150, 'metric')).toBe('−150 g');
		expect(formatWeight(null, 'metric')).toBe('–');
	});

	it('formats US: oz, then lb from 1 lb', () => {
		expect(formatWeight(250, 'us')).toBe('8.8 oz');
		expect(formatWeight(340.19, 'us')).toBe('12 oz');
		expect(formatWeight(18, 'us')).toBe('0.63 oz');
		expect(formatWeight(1000, 'us')).toBe('2.2 lb');
		expect(formatWeight(1002.5, 'us')).toBe('2.21 lb');
		expect(formatWeight(2267.96, 'us')).toBe('5 lb');
		expect(formatWeight(453, 'us')).toBe('1 lb'); // 15.98 oz reads 16.0: a pound
		expect(formatWeight(-150, 'us')).toBe('−5.3 oz');
		expect(formatWeight(undefined, 'us')).toBe('–');
	});

	it('formats a roast weight on a scale more closely', () => {
		expect(formatWeight(85.75, 'metric', { precise: true })).toBe('85.75 g');
		expect(formatWeight(100, 'metric', { precise: true })).toBe('100 g');
		expect(formatWeight(85.75, 'us', { precise: true })).toBe('3.02 oz');
	});

	it('parses the CLI forms', () => {
		expect(parseWeight('250 g', 'metric')).toBe(250);
		expect(parseWeight('250g', 'us')).toBe(250);
		expect(parseWeight('85.75', 'metric')).toBe(85.75);
		expect(parseWeight('1.5kg', 'us')).toBe(1500);
		expect(parseWeight('1,5 kg', 'metric')).toBe(1500);
		expect(parseWeight('8 oz', 'metric')).toBe(226.8);
		expect(parseWeight('2 lb', 'metric')).toBe(907.2);
		expect(parseWeight('0.5 lb', 'metric')).toBe(226.8);
		expect(parseWeight('5 LBS', 'metric')).toBe(2268);
		expect(parseWeight('.5lb', 'metric')).toBe(226.8);
	});

	it('reads a bare number as the small unit', () => {
		expect(parseWeight('100', 'metric')).toBe(100);
		expect(parseWeight('8', 'us')).toBe(226.8);
		expect(smallUnit('metric')).toBe('g');
		expect(smallUnit('us')).toBe('oz');
	});

	it('rejects junk and negatives unless signed', () => {
		for (const t of ['', '  ', 'abc', '2 stone', '1.2.3', '5 g g', 'g', '-']) {
			expect(typeof parseWeight(t, 'metric')).toBe('string');
		}
		expect(typeof parseWeight('-150', 'metric')).toBe('string');
		expect(parseWeight('-150', 'metric', { signed: true })).toBe(-150);
		expect(parseWeight('−2 oz', 'metric', { signed: true })).toBe(-56.7);
		expect(parseWeight('0', 'metric')).toBe(0);
	});

	it('fills a field with the stored weight, and parses back close to it', () => {
		expect(weightInputText(100, 'metric')).toBe('100');
		expect(weightInputText(85.75, 'metric')).toBe('85.75');
		expect(weightInputText(null, 'us')).toBe('');
		expect(weightInputText(100, 'us')).toBe('3.53 oz');
		expect(weightInputText(1000, 'us')).toBe('2.205 lb');
		for (const g of [18, 100, 250, 1000, 5000]) {
			expect(parseWeight(weightInputText(g, 'us'), 'us') as number).toBeCloseTo(g, -0.5);
		}
	});

	it('prices per kg or per lb', () => {
		expect(perBigUnit(20, 'metric')).toBe(20);
		expect(perBigUnit(20, 'us')).toBeCloseTo(9.07, 2);
	});
});
