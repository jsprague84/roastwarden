import { describe, expect, it } from 'vitest';
import type { ModelView, Prediction } from '#lib/api/predict.js';
import {
	landmarkText,
	modelLines,
	predictionOnGrid,
	rorScoreText,
	rorWarningText,
	scoreText
} from './prediction.js';

const p: Prediction = {
	t: 300,
	bt: 180,
	plan: 'drive',
	mode: 'plan',
	reason: 'test',
	points: [
		{ t: 305, bt: 181, low: 180, high: 182, heater: 90 },
		{ t: 310, bt: 182, low: 180, high: 184, heater: 90 }
	],
	bt_60: 190,
	bt_120: 200,
	band_120: 3
};

describe('predictionOnGrid', () => {
	it('starts at BT now with no band and interpolates to its last point', () => {
		const g = predictionOnGrid(p, [290, 300, 302.5, 310, 320], 0);
		expect(g.mid).toEqual([null, 180, 180.5, 182, null]);
		expect(g.low[2]).toBeCloseTo(180);
		expect(g.high[2]).toBeCloseTo(181);
		expect(g.high[3]).toBe(184);
	});
	it('moves with CHARGE on the scope time base', () => {
		const g = predictionOnGrid(p, [310, 320], 10);
		expect(g.mid).toEqual([180, 182]);
	});
	it('is empty without a prediction', () => {
		expect(predictionOnGrid(null, [1, 2], 0).mid).toEqual([null, null]);
	});
});

describe('words', () => {
	it('gives a range only when it is wider than a second or two', () => {
		expect(landmarkText({ t: 451, low: 440, high: 465 })).toBe('7:31 (7:20–7:45)');
		expect(landmarkText({ t: 605, low: 605, high: 605 })).toBe('10:05');
		expect(landmarkText(null)).toBeNull();
	});
	it('scores two minutes ahead against the projection', () => {
		const h = (rmse: number) => [{ horizon_s: 120, rmse, p90: 0, bias: 0, n: 3 }];
		expect(scoreText({ model: h(1.94), projection: h(30.1), predictions: 40 })).toBe(
			'2 min ahead RMS 1.9 °C (projection 30.1 °C), 40 predictions'
		);
		expect(scoreText({ model: [], projection: [], predictions: 0 })).toBeNull();
	});
	it('describes a model before it has roasts', () => {
		const v: ModelView = {
			device_id: 'v',
			source: 'prior',
			model: { tau_s: 8, gain: 2.6, knot_from_c: 20, knot_step_c: 20, drift: [] },
			settles: [{ heater_pct: 100, fan_pct: 100, bt: 184.2 }],
			recent: [],
			pooled: { roasts: 0 }
		};
		const l = modelLines(v);
		expect(l.fitted).toMatch(/virtual roaster/);
		expect(l.settles).toBe('100/100 % → 184 °C');
		expect(l.live).toBeNull();
	});
});

describe('RoR warnings', () => {
	it('names a live warning and when', () => {
		expect(rorWarningText({ kind: 'stall', t: 545, seen: false, since: 500, reason: 'x' })).toBe(
			'BT stall ~9:05'
		);
		expect(rorWarningText({ kind: 'flick', t: 505, seen: true, since: 500, reason: 'x' })).toBe(
			'RoR flick now'
		);
	});

	it('scores warnings against what the RoR did', () => {
		expect(
			rorScoreText({
				ror_warnings: [
					{
						kind: 'flick',
						warned_at: 455,
						expected_t: 519,
						seen: false,
						reason: 'r',
						last_t: 515,
						came_t: 520,
						lead_s: 65
					}
				],
				ror_events: [
					{ kind: 'flick', t: 520, seen: true, since: 520, reason: 'r' },
					{ kind: 'stall', t: 560, seen: true, since: 560, reason: 'r' }
				]
			})
		).toBe('RoR flick warned 7:35, came 8:40 (65 s ahead); BT stall at 9:20, not warned');
		expect(
			rorScoreText({
				ror_warnings: [
					{ kind: 'crash', warned_at: 300, expected_t: 330, seen: false, reason: 'r', last_t: 310 }
				]
			})
		).toBe('RoR crash warned 5:00, did not come');
		expect(rorScoreText({})).toBeNull();
	});
});
