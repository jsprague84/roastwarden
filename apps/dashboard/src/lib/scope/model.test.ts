import { describe, expect, it } from 'vitest';
import type { Prediction } from '#lib/api/predict.js';
import type { ProfileWithPoints } from '#lib/api/profiles.js';
import type { Telemetry } from '#lib/api/telemetry.js';
import {
	BAND,
	CH0,
	MODEL_HI,
	MODEL_LO,
	SERIES,
	TOGGLEABLE,
	cursorAt,
	scopeModel,
	scopeReadout,
	type ScopeInput
} from './model.js';
import { futureOffsets, type HistoryEntry, type Num } from './scopeData.js';

const T0 = 1_700_000_000_000;

/** A roast charged at 180 °C, BT falling to 95 °C at 1:00 then rising 0.4 °C/s, one sample a second. */
function history(seconds: number): HistoryEntry[] {
	const out: HistoryEntry[] = [];
	for (let i = 0; i <= seconds; i++) {
		const bt = i < 60 ? 180 - (85 * i) / 60 : 95 + 0.4 * (i - 60);
		const telemetry: Telemetry = {
			beanTemp: bt,
			envTemp: bt + 30,
			heater: i < 60 ? 100 : 70,
			fan: 60,
			setpoint: 200,
			controlMode: 1,
			heaterEnable: 1,
			rateOfRise: i < 60 ? -85 : 24
		};
		out.push({ timestamp: T0 + i * 1000, telemetry });
	}
	return out;
}

const profile: ProfileWithPoints = {
	id: 'p1',
	name: 'Light',
	description: null,
	target_total_time: 600,
	target_first_crack: 480,
	target_end_temp: 215,
	charge_temp: 180,
	roast_level: 'light',
	created_at: '',
	points: [
		{
			id: 'a',
			time_seconds: 0,
			target_temp: 180,
			fan_speed: null,
			notes: null,
			target_env_temp: null
		},
		{
			id: 'b',
			time_seconds: 60,
			target_temp: 95,
			fan_speed: null,
			notes: null,
			target_env_temp: null
		},
		{
			id: 'c',
			time_seconds: 600,
			target_temp: 215,
			fan_speed: null,
			notes: null,
			target_env_temp: null
		}
	],
	events: [
		{
			id: 'e',
			profile_id: 'p1',
			created_at: '',
			trigger_kind: 'time',
			at_seconds: 300,
			at_bt: null,
			action: 'fan',
			value: 50,
			label: null
		}
	]
};

const base: ScopeInput = {
	history: history(240),
	sessionStartMs: T0,
	landmarks: [
		{ type: 'charge', elapsed_seconds: 0, temperature: 180 },
		{ type: 'turning_point', elapsed_seconds: 60, temperature: 95 },
		{ type: 'custom', elapsed_seconds: 100 }
	],
	previewing: false,
	profile,
	replay: null,
	prediction: null,
	heaterLine: 'actual',
	channels: [],
	targetDtr: 20,
	view: 'roast'
};

describe('the scope model', () => {
	it('is null before the first sample', () => {
		expect(scopeModel({ ...base, history: [] })).toBeNull();
	});

	it('lays the series out as the chart expects: live samples, then the future', () => {
		const m = scopeModel(base)!;
		expect(m.n).toBe(241);
		expect(m.nowSec).toBe(240);
		expect(m.chargeAtOrigin).toBe(true);
		const x = m.data[0] as number[];
		const ahead = futureOffsets(
			240,
			120,
			10,
			Array.from({ length: 601 }, (_, i) => i)
		);
		expect(x.length).toBe(241 + ahead.length);
		expect(m.data.length).toBe(CH0); // 17 fixed series, no channels
		for (const k of TOGGLEABLE) expect(m.data[SERIES[k]].length).toBe(x.length);
		// live BT as sent; the profile's BT and the Δ band only where live BT exists
		expect(m.data[SERIES.bt][0]).toBe(180);
		expect(m.data[SERIES.bt][300]).toBeNull();
		expect(m.data[SERIES.profBt][300]).not.toBeNull();
		expect(m.data[BAND][240]).not.toBeNull();
		expect(m.data[BAND][241]).toBeNull();
		// no prediction asked: the model's band is empty
		expect((m.data[MODEL_LO] as Num[]).every((v) => v == null)).toBe(true);
		expect((m.data[MODEL_HI] as Num[]).every((v) => v == null)).toBe(true);
	});

	it('frames the roast, marks the landmarks that are not notes, and bands the phases', () => {
		const m = scopeModel(base)!;
		expect(m.xRange[0]).toBe(-15);
		expect(m.xRange[1]).toBeGreaterThanOrEqual(660);
		expect(m.marks.map((k) => k.type)).toEqual(['charge', 'turning_point']);
		expect(m.marks[1]).toEqual({ t: 60, type: 'turning_point', temp: 95 });
		expect(m.bands.map((b) => b.kind)).toContain('drying');
		expect(m.ticks).toHaveLength(1);
		expect(m.ticks[0].t).toBe(300);
	});

	it('reads the numbers now: BT against the profile, the projection and the tracking', () => {
		const m = scopeModel(base)!;
		const s = m.stats;
		expect(s.bt).toBeCloseTo(167, 0);
		expect(s.ror).toBe(24);
		// the profile is at 95 + 120 × (180 / 540) = 135 °C at 4:00: BT is 32 °C ahead of it
		expect(s.delta).toBeCloseTo(32, 6);
		expect(s.proj60).toBeGreaterThan(s.bt);
		expect(s.fcTarget).toBe(196);
		expect(s.etaFc).toBeGreaterThan(0);
		expect(s.trackRms).toBeGreaterThan(0);
		expect(s.dev).toBeNull(); // the development guide starts at first crack
		expect(s.devWindow).toBeNull();
		expect(s.prediction).toBeNull();
	});

	it('guides the development once first crack is marked, with the expected window', () => {
		const m = scopeModel({
			...base,
			landmarks: [
				...base.landmarks,
				{ type: 'first_crack_start', elapsed_seconds: 200, temperature: 190 }
			]
		})!;
		expect(m.stats.dev).not.toBeNull();
		expect(m.stats.dev!.devSec).toBe(40);
		expect(m.stats.dev!.targetDtr).toBe(20); // the profile's: FC at 8:00 of 10:00
		expect(m.stats.devWindow).not.toBeNull();
		expect(m.stats.devWindow!.from).toBe(200 + m.stats.devWindow!.lo);
		expect(m.stats.devWindow!.hi).toBeGreaterThan(m.stats.devWindow!.lo);
	});

	it('stops projecting once DROP is marked, and shifts the profile by the drive alignment', () => {
		const dropped = scopeModel({
			...base,
			landmarks: [...base.landmarks, { type: 'drop', elapsed_seconds: 230, temperature: 165 }]
		})!;
		expect(dropped.stats.proj60).toBeNull();
		expect(dropped.stats.dev).toBeNull();
		expect((dropped.data[SERIES.projBt] as Num[]).every((v) => v == null)).toBe(true);
		const aligned = scopeModel({
			...base,
			replay: {
				align: {
					on: 'turning_point',
					applied: true,
					offset_seconds: 30,
					offset_c: null,
					profile_landmark_t: 60
				},
				config: { trigger: 'time' } as never
			}
		})!;
		expect(aligned.ticks[0].t).toBe(330);
	});

	it('draws the prediction on the grid between CHARGE and DROP, and not outside them', () => {
		const prediction: Prediction = {
			t: 240,
			bt: 167,
			plan: 'held',
			mode: 'recorded',
			reason: 'test',
			points: [
				{ t: 300, bt: 190, low: 185, high: 195, heater: 70 },
				{ t: 360, bt: 205, low: 198, high: 212, heater: 70 }
			],
			bt_60: 190,
			bt_120: 205,
			band_120: 7
		};
		const m = scopeModel({ ...base, prediction })!;
		expect(m.stats.prediction).toBe(prediction);
		const x = m.data[0] as number[];
		const at = (t: number, series: number) => m.data[series][x.indexOf(t)];
		expect(at(240, SERIES.model)).toBe(167);
		expect(at(300, SERIES.model)).toBe(190);
		expect(at(300, MODEL_LO)).toBe(185);
		expect(at(300, MODEL_HI)).toBe(195);
		expect(at(100, SERIES.model)).toBeNull();
		const noCharge = scopeModel({ ...base, prediction, landmarks: [] })!;
		expect(noCharge.stats.prediction).toBeNull();
		expect((noCharge.data[SERIES.model] as Num[]).every((v) => v == null)).toBe(true);
		const dropped = scopeModel({
			...base,
			prediction,
			landmarks: [...base.landmarks, { type: 'drop', elapsed_seconds: 230 }]
		})!;
		expect(dropped.stats.prediction).toBeNull();
		expect((dropped.data[SERIES.model] as Num[]).every((v) => v == null)).toBe(true);
	});

	it('previews a profile from now with no roast open: no Δ, a frame of its own', () => {
		const m = scopeModel({ ...base, sessionStartMs: null, landmarks: [], previewing: true })!;
		expect(m.chargeAtOrigin).toBe(false);
		expect(m.stats.delta).toBeNull();
		expect(m.marks).toEqual([]);
		expect(m.xRange[0]).toBeLessThan(0);
	});

	it('appends a declared channel after the fixed series, converted when it is a temperature', () => {
		const h = history(10).map((e) => ({
			...e,
			telemetry: { ...e.telemetry, t3: 50, pressure: 2.5 } as Telemetry
		}));
		const m = scopeModel({
			...base,
			history: h,
			channels: [
				{ key: 't3', label: 'Inlet', unit: 'C', max: null },
				{ key: 'pressure', label: 'Pressure', unit: 'bar', max: 4 }
			]
		})!;
		expect(m.data.length).toBe(CH0 + 2);
		expect(m.data[CH0][0]).toBe(50);
		expect(m.data[CH0 + 1][0]).toBe(2.5);
		expect(m.chLines[1].top).toBe(4);
	});
});

describe('the readout and the cursor', () => {
	it('reads now, saying how long since charge', () => {
		const m = scopeModel(base)!;
		const r = scopeReadout(m, null, false);
		expect(r.title).toBe('4:00 since charge');
		expect(r.bt).toBeCloseTo(167, 0);
		expect(r.ror).toBe(24);
		expect(r.trackRms).toBeGreaterThan(0);
		expect(
			scopeReadout(
				scopeModel({ ...base, sessionStartMs: null, landmarks: [], previewing: true })!,
				null,
				true
			).title
		).toBe('Preview: profile from now');
	});

	it('reads the hovered sample instead, with its Δ from the profile', () => {
		const m = scopeModel(base)!;
		const c = cursorAt(m, 120)!;
		expect(c.t).toBe(120);
		expect(c.vals.bt).toBe(m.data[SERIES.bt][120]);
		const r = scopeReadout(m, c, false);
		expect(r.title).toBe('at 2:00');
		expect(r.delta).toBeCloseTo((c.vals.bt as number) - (c.vals.profBt as number), 6);
		expect(r.trackRms).toBeNull();
		expect(cursorAt(m, 500)).toBeNull(); // past the live samples
		expect(cursorAt(m, null)).toBeNull();
	});
});
