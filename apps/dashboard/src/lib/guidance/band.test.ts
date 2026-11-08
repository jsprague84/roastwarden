import { describe, expect, it } from 'vitest';
import { bandGeometry, bandHelp } from './band.js';
import { judge } from './judge.js';
import { expectationsFor } from './expect.js';

describe('bandGeometry', () => {
	it('puts the range in the middle half and the value on the bar', () => {
		const g = bandGeometry([14, 16], 15);
		expect(g.lo).toBeCloseTo(25);
		expect(g.hi).toBeCloseTo(75);
		expect(g.at).toBeCloseTo(50);
	});

	it('widens the scale for a value far outside', () => {
		const g = bandGeometry([14, 16], 21.7);
		expect(g.at!).toBeLessThanOrEqual(100);
		expect(g.at!).toBeGreaterThan(g.hi);
		expect(bandGeometry([14, 16], null).at).toBeNull();
	});
});

describe('bandHelp', () => {
	it('names who expects what, the basis and linked sources', () => {
		const exp = expectationsFor({ roast_level: 'light' }, null);
		const h = bandHelp(judge(21.7, exp.weight_loss_pct)!, '14–16 %', 'Light', [
			'Loss like this suggests a dark roast.'
		]);
		expect(h.help.line).toBe('Light expects 14–16 %. Above.');
		expect(h.help.more).toMatch(/Mondry/);
		expect(h.help.more).toMatch(/Roastwarden default/);
		expect(h.help.more).toMatch(/suggests a dark roast/);
		expect(h.links[0].url).toMatch(/^https:/);
	});
});
