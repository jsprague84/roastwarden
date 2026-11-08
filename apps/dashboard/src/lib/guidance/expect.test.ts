import { describe, expect, it } from 'vitest';
import { expectationsFor, historyBand, historyNote, MIN_HISTORY } from './expect.js';
import { historyRow } from './fixtures.js';

describe('historyBand', () => {
	it('is mean ± max(2σ, floor), and needs 3 values', () => {
		expect(historyBand([200, 200], 2)).toBeNull();
		expect(historyBand([200, 200, 200], 2)).toEqual([198, 202]);
		const [lo, hi] = historyBand([190, 200, 210], 2)!;
		expect(lo).toBeCloseTo(180);
		expect(hi).toBeCloseTo(220);
	});

	it('never starts below zero: the smallest value seen instead', () => {
		// Total times 0:30, 10:00, 10:10: mean ± 2σ would start at −4:11
		const [lo, hi] = historyBand([30, 600, 610], 15)!;
		expect(lo).toBe(30);
		expect(hi).toBeGreaterThan(610);
		expect(historyBand([-3, 1, 2], 0.5)![0]).toBe(0);
	});
});

describe('expectationsFor', () => {
	it('takes the profile’s override, else the level default', () => {
		const e = expectationsFor(
			{ roast_level: 'light', expectations: { development_s: [70, 110] } },
			null
		);
		expect(e.level).toBe('light');
		expect(e.development_s).toMatchObject({ range: [70, 110], source: 'profile' });
		expect(e.weight_loss_pct).toMatchObject({ range: [14, 16], source: 'default', derived: true });
		expect(e.color_ground).toMatchObject({ range: [70, 80], source: 'default' });
		expect(e.weight_loss_pct!.basis).toMatch(/^Light: Mondry/);
	});

	it('falls back to the session’s target level', () => {
		expect(expectationsFor({ roast_level: null }, { target_roast_level: 'medium' }).level).toBe(
			'medium'
		);
		expect(expectationsFor(null, { target_roast_level: 'City+' }).level).toBeNull();
	});

	it('shifts the loss range for green moisture', () => {
		const e = expectationsFor({ roast_level: 'light' }, { moisture_green: 12 });
		expect(e.weight_loss_pct!.range).toEqual([15.5, 17.5]);
		expect(e.weight_loss_pct!.basis).toMatch(/12 % green moisture/);
	});

	it('judges temperatures and times from own history only, from 3 roasts', () => {
		const two = [historyRow('a'), historyRow('b')];
		let e = expectationsFor({ roast_level: 'light' }, { id: 'now' }, two);
		expect(e.fc_bt).toBeNull();
		expect(e.historyCount).toBe(2);
		const three = [...two, historyRow('c', { fc_bt: 202, total_s: 700 })];
		e = expectationsFor({ roast_level: 'light' }, { id: 'now' }, three);
		expect(e.fc_bt).toMatchObject({ source: 'history', n: 3 });
		expect(e.total_s!.range[0]).toBeLessThan(660);
		// The judged roast is not its own reference.
		e = expectationsFor({ roast_level: 'light' }, { id: 'c' }, three);
		expect(e.fc_bt).toBeNull();
	});

	it('uses own history for loss and colour when there is no level', () => {
		const h = [
			historyRow('a'),
			historyRow('b'),
			historyRow('c', { color_ground: 115, color_scale: 'tonino' })
		];
		const e = expectationsFor(null, null, h);
		expect(e.weight_loss_pct).toMatchObject({ source: 'history', range: [14.5, 15.5] });
		expect(e.color_ground!.source).toBe('history');
		expect(e.development_s).toBeNull();
	});

	it('says when history is too thin', () => {
		expect(historyNote(1)).toBe(
			'1 other roast of this profile on this roaster; temperatures and times are judged from 3.'
		);
		expect(historyNote(MIN_HISTORY)).toBeNull();
	});
});
