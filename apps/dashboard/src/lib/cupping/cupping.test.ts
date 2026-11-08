import { describe, expect, it } from 'vitest';
import {
	ATTRIBUTES,
	band,
	capDefects,
	cupsFromScore,
	defects,
	freshScores,
	scoreFromCups,
	summarize,
	total
} from './cupping.js';

describe('the classic SCA form', () => {
	it('starts quality at 6 and the cup attributes at 10', () => {
		const s = freshScores();
		expect(s.acidity).toBe(6);
		expect(s.uniformity).toBe(10);
		expect(s.clean_cup).toBe(10);
		expect(s.sweetness).toBe(10);
		// 6 × 7 + 10 × 3
		expect(total(s)).toBe(72);
	});

	it('has the ten attributes in the protocol order', () => {
		expect(ATTRIBUTES.map((a) => a.key)).toEqual([
			'fragrance_aroma',
			'flavor',
			'aftertaste',
			'acidity',
			'body',
			'balance',
			'uniformity',
			'clean_cup',
			'sweetness',
			'overall'
		]);
	});

	it('takes defects off the total and bands the final score', () => {
		const s = { ...freshScores() };
		for (const a of ATTRIBUTES) if (a.kind === 'quality') s[a.key] = 7.5;
		s.clean_cup = 8;
		// 7.5 × 7 + 10 + 8 + 10 = 80.5; one taint and one fault: 6 off
		expect(summarize(s, 1, 1)).toEqual({
			total: 80.5,
			defects: 6,
			final: 74.5,
			band: 'below specialty'
		});
		expect(summarize(s, 0, 0).band).toBe('very good');
		expect(band(85)).toBe('excellent');
		expect(band(90)).toBe('outstanding');
		expect(band(84.75)).toBe('very good');
		expect(defects(2, 1)).toBe(8);
	});

	it('has no total until all ten are scored', () => {
		expect(total({ acidity: 7 })).toBeNull();
		expect(summarize({ acidity: 7 }, 1, 0)).toEqual({
			total: null,
			defects: 2,
			final: null,
			band: null
		});
	});

	it('scores cup attributes 2 a cup', () => {
		expect(cupsFromScore(10)).toBe(5);
		expect(cupsFromScore(8)).toBe(4);
		expect(cupsFromScore(0)).toBe(0);
		expect(scoreFromCups(3)).toBe(6);
		expect(scoreFromCups(9)).toBe(10);
	});

	it('keeps taint and fault cups within five', () => {
		expect(capDefects('taint', 3, 3)).toEqual({ taint: 3, fault: 2 });
		expect(capDefects('fault', 3, 3)).toEqual({ taint: 2, fault: 3 });
		expect(capDefects('taint', 1, 1)).toEqual({ taint: 1, fault: 1 });
		expect(capDefects('fault', 0, 7)).toEqual({ taint: 0, fault: 5 });
	});
});
