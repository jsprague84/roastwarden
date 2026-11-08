import { describe, expect, it } from 'vitest';
import { roastSummary } from '#lib/roast/report.js';
import { expectationsFor, historyNote } from './expect.js';
import {
	factsFrom,
	guidanceSummary,
	judgeRoast,
	mmss,
	outsideColumns,
	readRoast
} from './summary.js';
import { batch1, batch1Events, historyRow, lightEspresso } from './fixtures.js';

const sum = roastSummary(batch1Events, []);

describe('guidanceSummary', () => {
	it('reads batch #1 as the worked example says', () => {
		const facts = factsFrom(batch1, sum);
		expect(facts.development_s).toBeCloseTo(246.1, 1);
		expect(facts.total_s).toBeCloseTo(713.8, 1);
		const exp = expectationsFor(lightEspresso, batch1, [historyRow(batch1.id)]);
		const lines = guidanceSummary({ facts, exp, profileFc: lightEspresso.target_first_crack });
		expect(lines).toEqual([
			'Weight loss 21.7 %: Light expects 14–16 %. Loss like this usually means a dark roast (about Agtron 25–35); a colour reading would confirm it.',
			'Development 4:06 (DTR 34 %): Light expects 1:00–2:00. Shorten the profile after first crack.',
			'First crack 1:12 earlier than the profile (7:48 vs 9:00).'
		]);
		// Nothing about temperatures: one roast of the profile, judged from three.
		expect(exp.fc_bt).toBeNull();
		expect(historyNote(exp.historyCount)).toMatch(/^No other roasts/);
	});

	it('is empty for a roast within everything', () => {
		const facts = {
			...factsFrom(batch1, sum),
			weight_loss_pct: 15,
			development_s: 100,
			dtr_pct: 15,
			fc_s: 545,
			color_ground: 74,
			color_scale: 'agtron'
		};
		const exp = expectationsFor(lightEspresso, batch1);
		expect(guidanceSummary({ facts, exp, profileFc: 540 })).toEqual([]);
	});

	it('says nothing about loss without weights', () => {
		const s = { ...batch1, roasted_weight: null, weight_loss_pct: null };
		const lines = guidanceSummary({
			facts: factsFrom(s, sum),
			exp: expectationsFor(lightEspresso, s),
			profileFc: 540
		});
		expect(lines.some((l) => l.startsWith('Weight loss'))).toBe(false);
		expect(lines[0]).toMatch(/^Development 4:06/);
	});

	it('leads with colour, and drops the "confirm" clause once colour is measured', () => {
		const s = { ...batch1, color_ground: 31, color_scale: 'agtron' };
		const lines = guidanceSummary({
			facts: factsFrom(s, sum),
			exp: expectationsFor(lightEspresso, s),
			profileFc: 540
		});
		expect(lines[0]).toBe('Ground colour Agtron 31: Light expects 70–80, so it is darker (dark).');
		expect(lines[1]).toBe(
			'Weight loss 21.7 %: Light expects 14–16 %. Loss like this usually means a dark roast (about Agtron 25–35).'
		);
		expect(lines).toHaveLength(3);
	});

	it('judges temperatures against three or more of this profile’s roasts', () => {
		const h = ['a', 'b', 'c'].map((id) => historyRow(id, { fc_bt: 200, drop_bt: 212 }));
		const s = { ...batch1, roasted_weight: 85, weight_loss_pct: 15 };
		const facts = { ...factsFrom(s, sum), development_s: 100, fc_s: 540 };
		const exp = expectationsFor(lightEspresso, s, h);
		const j = judgeRoast(facts, exp);
		expect(j.fc_bt?.state).toBe('below');
		const lines = guidanceSummary({ facts, exp, profileFc: 540 });
		expect(lines).toContain(
			'First crack at 194.5 °C: this profile’s last 3 roasts here ran 198.0–202.0 °C.'
		);
	});
});

describe('mmss', () => {
	it('formats seconds', () => {
		expect(mmss(246.1)).toBe('4:06');
		expect(mmss(60)).toBe('1:00');
	});
});

describe('readRoast', () => {
	it('reads batch #1 with its profile, and notes the thin history', () => {
		const r = readRoast({
			session: { ...batch1, profile: lightEspresso },
			events: batch1Events,
			telemetry: [],
			history: [historyRow(batch1.id)]
		});
		expect(r.lines).toHaveLength(3);
		expect(r.j.weight_loss_pct?.state).toBe('above');
		expect(r.note).toBe(
			'No other roasts of this profile on this roaster yet; temperatures and times are judged from 3.'
		);
		expect(r.lossSuggests).toBe(
			'A loss of 21.7 % usually means a dark roast (about Agtron 25–35).'
		);
	});

	it('asks for a level when the profile has none', () => {
		const r = readRoast({
			session: { ...batch1, profile: { roast_level: null, target_first_crack: 540 } },
			events: batch1Events,
			telemetry: []
		});
		expect(r.note).toMatch(/^Set a roast level on the profile/);
		expect(r.lines).toEqual(['First crack 1:12 earlier than the profile (7:48 vs 9:00).']);
	});
});

describe('outsideColumns', () => {
	it('flags batch #1’s loss and development against Light', () => {
		expect([...outsideColumns(batch1, lightEspresso)].sort()).toEqual(['dtr', 'loss']);
		expect(outsideColumns(batch1, { roast_level: null }).size).toBe(0);
		expect(
			outsideColumns(
				{ ...batch1, weight_loss_pct: 15, development_time_ratio: 0.12 },
				lightEspresso
			).size
		).toBe(0);
	});
});
