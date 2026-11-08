import { describe, expect, it } from 'vitest';
import { planFromCurve, readPlan, reviewProfile, type Plan } from './profile.js';
import { defaultsFor, isLevel } from './references.js';
import { historyRow } from './fixtures.js';

/** The seed Light espresso: FC 9:00 at 200 °C, drop 11:30 (migration 015). */
const espresso: Plan = planFromCurve({ fc_s: 540, fc_bt: 200, dry_s: 278, total_s: 690 });
const light = { roast_level: 'light' };

describe('readPlan', () => {
	it('judges the planned development and names the expected results', () => {
		const r = readPlan(espresso, light);
		expect(r.development_s).toBe(150);
		expect(r.development?.state).toBe('above');
		expect(r.expected).toBe(
			'Light: weight loss 14–16 %, ground Agtron 70–80, development 1:00–2:00.'
		);
		expect(r.phases).toBe('Drying 40 % · Maillard 38 % · development 22 %');
		expect(readPlan(espresso, null).development).toBeNull();
		expect(readPlan(espresso, null).expected).toBeNull();
	});
});

describe('the seed profiles sit inside their own level (except Light espresso, by design)', () => {
	// id: [fc_s, total_s, level] from migrations 015, 021 and 051 (the drum seeds).
	const seeds: Record<string, [number, number, string]> = {
		'drum-light-filter': [495, 585, 'light'],
		'drum-medium-omni': [525, 660, 'medium'],
		'fluidbed-light': [465, 570, 'light'],
		'fluidbed-medium': [510, 660, 'medium'],
		'fluidbed-nordic': [450, 510, 'extremely_light'],
		'fluidbed-cupping': [480, 600, 'medium_light'],
		'fluidbed-espresso-short': [510, 615, 'light'],
		'fluidbed-omni': [495, 615, 'medium_light']
	};
	for (const [id, [fc, total, level]] of Object.entries(seeds)) {
		it(id, () => {
			expect(isLevel(level)).toBe(true);
			const r = readPlan(planFromCurve({ fc_s: fc, fc_bt: 200, dry_s: null, total_s: total }), {
				roast_level: level
			});
			expect(r.development?.state, `development ${r.development_s} s`).toBe('within');
			expect(r.dtr?.state, `DTR ${r.dtr_pct}`).toBe('within');
		});
	}
	it('the cupping roast is inside the SCA 8–12 min window', () => {
		expect(600).toBeGreaterThanOrEqual(480);
		expect(600).toBeLessThanOrEqual(720);
		expect(defaultsFor('medium_light').color_ground!.range).toEqual([60, 70]);
	});
});

describe('reviewProfile', () => {
	it('reads batch #1 against the Light espresso profile', () => {
		const h = [
			historyRow('b1', {
				fc_s: 467.7,
				fc_bt: 194.5,
				drop_s: 713.8,
				total_s: 713.8,
				development_s: 246.1,
				weight_loss_pct: 21.7,
				color_ground: null
			})
		];
		const r = reviewProfile(espresso, light, h);
		expect(r.n).toBe(1);
		expect(r.lines).toEqual([
			'First crack came 1:12 earlier than the profile (7:48 vs 9:00) and at 194.5 °C, 5.5 °C below its mark (1 roast). Set the profile’s FC mark to 195 °C so the scope’s countdown matches this probe.',
			'Development ran 4:06 (1 roast); Light expects 1:00–2:00. The profile itself plans 2:30: shorten it after first crack.',
			'Weight loss averaged 21.7 % (1 roast); Light expects 14–16 %.'
		]);
		expect(r.rows[0].fcDelta).toBeCloseTo(-72.3, 1);
		expect(r.rows[0].development?.state).toBe('above');
	});

	it('says to drop sooner when the plan is fine but the roasts run long', () => {
		const short = planFromCurve({ fc_s: 510, fc_bt: 200, dry_s: null, total_s: 615 });
		const h = ['a', 'b'].map((id) =>
			historyRow(id, { fc_s: 510, fc_bt: 200, development_s: 150, weight_loss_pct: 15 })
		);
		expect(reviewProfile(short, light, h).lines).toEqual([
			'Development ran 2:30 (2 roasts); Light expects 1:00–2:00. The profile plans 1:45: drop sooner after first crack.'
		]);
	});

	it('is quiet when the roasts match', () => {
		const h = [
			historyRow('a', {
				fc_s: 540,
				fc_bt: 200,
				development_s: 110,
				weight_loss_pct: 15,
				color_ground: 74
			})
		];
		expect(reviewProfile(espresso, light, h).lines).toEqual([]);
		expect(reviewProfile(espresso, light, []).rows).toEqual([]);
	});
});
