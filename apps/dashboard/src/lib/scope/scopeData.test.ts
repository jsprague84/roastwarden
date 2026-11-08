import { describe, it, expect } from 'vitest';
import {
	viewRange,
	resolveOrigin,
	buildScopeSeries,
	phaseBands,
	phasePercents,
	fitLine,
	projectBt,
	projectRor,
	etaToTemp,
	profileCurve,
	recordedCurve,
	profileTicks,
	effectiveTrigger,
	alignToBase,
	eventFlags,
	fmtClock,
	stackLabels,
	channelLines,
	PROJECT_AFTER_CHARGE_SEC,
	type HistoryEntry
} from './scopeData.js';

function entry(
	tsMs: number,
	bt: number,
	extra: Partial<HistoryEntry['telemetry']> = {}
): HistoryEntry {
	return {
		timestamp: tsMs,
		telemetry: {
			beanTemp: bt,
			envTemp: bt + 20,
			heater: 50,
			fan: 100,
			setpoint: 200,
			controlMode: 1,
			heaterEnable: 1,
			...extra
		}
	};
}

describe('origin and series', () => {
	it('rebases to CHARGE when present, else session start, else first sample', () => {
		const hist = [entry(10_000, 100), entry(11_000, 101)];
		expect(resolveOrigin(hist, null, []).originMs).toBe(10_000);
		expect(resolveOrigin(hist, 5_000, []).originMs).toBe(5_000);
		const o = resolveOrigin(hist, 5_000, [{ type: 'charge', elapsed_seconds: 30 }]);
		expect(o.originMs).toBe(35_000);
		expect(o.chargeAtOrigin).toBe(true);
	});

	it('builds column series in seconds from origin', () => {
		const s = buildScopeSeries(
			[entry(35_000, 100, { rateOfRise: 8 }), entry(36_000, NaN, { setpoint: 0 })],
			35_000
		);
		expect(s.t).toEqual([0, 1]);
		expect(s.bt).toEqual([100, null]);
		expect(s.rorBt).toEqual([8, null]);
		expect(s.sp).toEqual([200, null]);
		expect(s.fan[0]).toBe(100);
	});
});

describe('phases', () => {
	const start = 0;
	const lm = [
		{ type: 'charge', elapsed_seconds: 10 },
		{ type: 'drying_end', elapsed_seconds: 250 },
		{ type: 'first_crack_start', elapsed_seconds: 490 }
	];
	it('derives drying/maillard and leaves development open until drop', () => {
		const origin = 10_000; // charge
		const bands = phaseBands(lm, start, origin, 600);
		expect(bands.map((b) => b.kind)).toEqual(['drying', 'maillard', 'development']);
		expect(bands[0]).toMatchObject({ from: 0, to: 240, open: false });
		expect(bands[2]).toMatchObject({ from: 480, to: 600, open: true });
		const pct = phasePercents(bands);
		expect(pct.drying + pct.maillard + pct.development).toBeGreaterThanOrEqual(99);
		expect(pct.drying).toBe(40);
	});
	it('returns nothing without a charge', () => {
		expect(phaseBands([{ type: 'drying_end', elapsed_seconds: 100 }], 0, 0, 500)).toEqual([]);
	});
});

describe('projections', () => {
	const t = Array.from({ length: 61 }, (_, i) => i);
	it('fits a line over the window', () => {
		const v = t.map((x) => 2 * x + 1);
		const fit = fitLine(t, v, 30)!;
		expect(fit.slope).toBeCloseTo(2);
		expect(fit.intercept).toBeCloseTo(121); // value at t=60
		expect(fitLine([1, 2], [1, 2], 30)).toBeNull();
	});
	it('projects BT with constant RoR linearly', () => {
		const bt = t.map((x) => 100 + x * 0.1); // 6 °C/min
		const ror = t.map(() => 6);
		const p = projectBt(t, bt, ror, 120)!;
		expect(p.from).toEqual([60, 106]);
		expect(p.at60).toBeCloseTo(112);
		expect(p.at120).toBeCloseTo(118);
		expect(etaToTemp(p, 115)).toBeCloseTo(90);
		expect(etaToTemp(p, 100)).toBeNull();
	});
	it('bends the projection when RoR is falling', () => {
		const ror = t.map((x) => 12 - x * 0.1); // falling 6 °C/min per minute
		const bt = t.map((x) => 100 + x * 0.15);
		const p = projectBt(t, bt, ror, 120)!;
		const linear = bt[60] + (ror[60] / 60) * 120;
		expect(p.at120).toBeLessThan(linear);
		// RoR reaches zero after 60 s (6 °C/min falling 6 per minute): BT holds, never drops.
		expect(p.at120).toBeCloseTo(p.at60, 5);
		expect(p.at60).toBeGreaterThan(bt[60]);
		const pr = projectRor(t, ror, 60)!;
		expect(pr.at60).toBeCloseTo(ror[60] - 6, 1);
	});
});

describe('profile and alignment', () => {
	it('resamples the profile to 1 s with profile t=0 at CHARGE (scope 0)', () => {
		const pts = [
			{ time_seconds: 0, target_temp: 100 },
			{ time_seconds: 10, target_temp: 200, fan_speed: 70 }
		];
		const c = profileCurve(pts);
		expect(c.t[0]).toBe(0);
		expect(c.t.length).toBe(11);
		expect(c.bt[5]).toBeCloseTo(150);
		expect(c.bt[10]).toBe(200);
	});
	it('shifts the profile by the replay align offset', () => {
		const c = profileCurve(
			[
				{ time_seconds: 0, target_temp: 100 },
				{ time_seconds: 10, target_temp: 200 }
			],
			30
		);
		expect(c.t[0]).toBe(30);
		expect(c.t[10]).toBe(40);
	});
	it('a CHARGE marked after the session start no longer pushes the profile early', () => {
		// Session started at 0, CHARGE marked 90 s in: the origin is CHARGE.
		const { originMs } = resolveOrigin([entry(0, 100)], 0, [
			{ type: 'charge', elapsed_seconds: 90 }
		]);
		expect(originMs).toBe(90_000);
		expect(
			profileCurve([
				{ time_seconds: 0, target_temp: 100 },
				{ time_seconds: 5, target_temp: 110 }
			]).t[0]
		).toBe(0);
	});
	it('aligns a curve to the base with interpolation and nulls outside', () => {
		const out = alignToBase([-1, 0, 0.5, 1, 2], [0, 1], [10, 20]);
		expect(out).toEqual([null, 10, 15, 20, null]);
	});
});

describe('event flags and clock', () => {
	it('flags heater, fan and setpoint changes after the first value', () => {
		const s = buildScopeSeries(
			[
				entry(0, 100),
				entry(1000, 101, { heater: 60 }),
				entry(2000, 102, { heater: 60, fan: 50 }),
				entry(3000, 103, { heater: 60, fan: 50, setpoint: 210 }),
				entry(4000, 104, { heater: 60, fan: 50, setpoint: 211 })
			],
			0
		);
		const flags = eventFlags(s);
		expect(flags).toEqual([
			{ t: 1, kind: 'heater', value: 60 },
			{ t: 2, kind: 'fan', value: 50 },
			{ t: 3, kind: 'sp', value: 210 }
		]);
	});
	it('formats negative clocks', () => {
		expect(fmtClock(-65)).toBe('-1:05');
		expect(fmtClock(600)).toBe('10:00');
	});
});

describe('recordedCurve', () => {
	it('places samples from CHARGE in scope seconds, nulls kept', () => {
		const c = recordedCurve({ interval_seconds: 1, bt: [200, null, 180], et: [250, 249, 248] });
		expect(c.t).toEqual([0, 1, 2]);
		expect(recordedCurve({ interval_seconds: 2, bt: [1, 2] }, -10).t).toEqual([-10, -8]);
		expect(c.bt).toEqual([200, null, 180]);
		expect(c.et).toEqual([250, 249, 248]);
		expect(c.fan).toEqual([null, null, null]); // missing channel
	});
});

describe('profileTicks', () => {
	// 200 at charge, TP 100 at 60 s, then 0.25 °C/s
	const curve = recordedCurve({
		interval_seconds: 1,
		bt: Array.from({ length: 461 }, (_, i) =>
			i < 60 ? 200 - (100 * i) / 60 : 100 + (i - 60) * 0.25
		)
	});
	const ev = (o: object) => ({
		trigger_kind: 'time' as const,
		at_seconds: null,
		at_bt: null,
		action: 'fan',
		value: 50,
		label: null,
		...o
	});
	it('maps time and BT triggers, sorted, skipping ones that never fire', () => {
		const ticks = profileTicks(
			[
				ev({ at_seconds: 300 }),
				ev({ trigger_kind: 'bt', at_bt: 150 }),
				ev({ trigger_kind: 'bt', at_bt: 260 })
			],
			curve
		);
		expect(ticks.map((t) => Math.round(t.t))).toEqual([260, 300]);
	});
	it('shifts time triggers by the align offset like the curve', () => {
		expect(profileTicks([ev({ at_seconds: 10 })], null, -4)[0].t).toBe(6);
	});
	it('places events by the replay trigger policy', () => {
		const both = ev({ at_seconds: 100, at_bt: 150 });
		expect(Math.round(profileTicks([both], curve, 0, 'profile')[0].t)).toBe(100);
		expect(Math.round(profileTicks([both], curve, 0, 'bt')[0].t)).toBe(260);
		// No BT on the event: falls back to its time.
		expect(profileTicks([ev({ at_seconds: 40 })], curve, 0, 'bt')[0].t).toBe(40);
		expect(effectiveTrigger(ev({ trigger_kind: 'bt', at_bt: 150 }), 'time')).toBe('bt');
	});
});

describe('viewRange', () => {
	const base = {
		dataMin: -5000,
		nowSec: 120,
		sessionStartSec: -90,
		profileEndSec: null,
		horizon: 120
	};

	it('frames a roast from just before the session start, at least 15 min wide', () => {
		expect(viewRange('roast', base)).toEqual([-105, 840]);
		// Past the frame it follows now + projection, on whole minutes.
		expect(viewRange('roast', { ...base, nowSec: 830 })).toEqual([-105, 960]);
		// A longer profile sets the end.
		expect(viewRange('roast', { ...base, profileEndSec: 900 })).toEqual([-105, 960]);
	});

	it('rolls with now when live, and falls back to live without a session', () => {
		expect(viewRange('live10', base)).toEqual([-480, 240]);
		expect(viewRange('live30', base)).toEqual([-1680, 240]);
		expect(viewRange('roast', { ...base, sessionStartSec: null })).toEqual([-480, 240]);
		expect(viewRange('all', base)).toEqual([-5000, 240]);
	});
});

describe('profileRor', () => {
	it('gives the slope in °C/min, smoothed across segment joins', async () => {
		const { profileCurve, profileRor } = await import('./scopeData.js');
		// 0→60 s: +30 °C (30 °C/min), then 60→180 s: +30 °C (15 °C/min).
		const c = profileCurve([
			{ time_seconds: 0, target_temp: 100, target_env_temp: null, fan_speed: null },
			{ time_seconds: 60, target_temp: 130, target_env_temp: null, fan_speed: null },
			{ time_seconds: 180, target_temp: 160, target_env_temp: null, fan_speed: null }
		] as never);
		const r = profileRor(c.t, c.bt);
		expect(r[30]).toBeCloseTo(30, 5);
		expect(r[120]).toBeCloseTo(15, 5);
		// At the join the window spans both segments: between the two.
		expect(r[60]!).toBeGreaterThan(15);
		expect(r[60]!).toBeLessThan(30);
		expect(profileRor([0], [100])[0]).toBeNull();
	});
});

describe('developmentGuide', () => {
	it('is null before first crack', async () => {
		const { developmentGuide } = await import('./scopeData.js');
		expect(
			developmentGuide({
				chargeSec: 0,
				fcSec: null,
				nowSec: 400,
				btNow: 190,
				btFc: null,
				ror: 8,
				targetDtr: 20
			})
		).toBeNull();
		expect(
			developmentGuide({
				chargeSec: 0,
				fcSec: 500,
				nowSec: 400,
				btNow: 190,
				btFc: null,
				ror: 8,
				targetDtr: 20
			})
		).toBeNull();
	});

	it('gives dev time, DTR, ΔT and the drop time for a target DTR', async () => {
		const { developmentGuide, profileDtr } = await import('./scopeData.js');
		// FC at 8:00 (480 s from a charge at scope 10 s), now 9:00 after charge.
		const g = developmentGuide({
			chargeSec: 10,
			fcSec: 490,
			nowSec: 550,
			btNow: 206,
			btFc: 200,
			ror: 6,
			targetDtr: 20,
			targetDropBt: 212
		})!;
		expect(g.devSec).toBe(60);
		expect(g.dtr).toBeCloseTo(11.11, 2);
		expect(g.dT).toBe(6);
		// 480 / 0.8 = 600 s after charge.
		expect(g.dropAtSec).toBeCloseTo(610, 6);
		expect(g.secsToDrop).toBeCloseTo(60, 6);
		expect(g.btAtDrop).toBeCloseTo(212, 6);
		expect(g.secsToDropBt).toBeCloseTo(60, 6);
		expect(profileDtr(540, 690)).toBeCloseTo(21.74, 2);
		expect(profileDtr(null, 690)).toBeNull();
	});
});

describe('profile overlay reach (virtual roaster overlay bug)', () => {
	it('previews a profile from "now" instead of the start of a long buffer', async () => {
		const { resolveOrigin } = await import('./scopeData.js');
		// Two hours of idle telemetry, no session: the profile used to start 2 h ago.
		const hist = [entry(0, 25), entry(3_600_000, 25), entry(7_200_000, 25)];
		expect(resolveOrigin(hist, null, [], true).originMs).toBe(7_200_000);
		expect(resolveOrigin(hist, null, [], false).originMs).toBe(0);
		// A session always wins over the preview.
		expect(resolveOrigin(hist, 5_000, [], true).originMs).toBe(5_000);
	});

	it('evaluates the whole profile ahead of now, not just the projection horizon', async () => {
		const { futureOffsets, profileCurve, alignToBase } = await import('./scopeData.js');
		const curve = profileCurve([
			{ time_seconds: 0, target_temp: 30 },
			{ time_seconds: 690, target_temp: 212 }
		]);
		const now = 40;
		const ks = futureOffsets(now, 120, 10, curve.t);
		// Projection steps first, then the profile to its end at 690 s.
		expect(ks.slice(0, 6)).toEqual([5, 10, 15, 20, 25, 30]);
		expect(Math.max(...ks)).toBe(650);
		expect(ks).toContain(120);
		// Strictly increasing, so uPlot accepts it.
		for (let i = 1; i < ks.length; i++) expect(ks[i]).toBeGreaterThan(ks[i - 1]);
		// The profile is drawn at the end of the roast frame.
		const x = ks.map((k) => now + k);
		const v = alignToBase(x, curve.t, curve.bt);
		expect(v[v.length - 1]).toBeCloseTo(212, 6);
		expect(v.filter((y) => y != null).length).toBe(x.length);
	});

	it('keeps only the projection steps once the profile is over', async () => {
		const { futureOffsets } = await import('./scopeData.js');
		expect(futureOffsets(900, 120, 10, [0, 300, 690])).toEqual([
			10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120
		]);
		expect(futureOffsets(0, 120, 10, null)).toHaveLength(12);
	});
});

describe('stackLabels', () => {
	it('puts labels that would overlap in the next free row', () => {
		const r = stackLabels(
			[
				{ x: 10, widths: [60] },
				{ x: 20, widths: [60] },
				{ x: 30, widths: [60] },
				{ x: 200, widths: [60] }
			],
			3,
			0,
			400
		);
		expect(r.map((p) => p?.row)).toEqual([0, 1, 2, 0]);
		expect(r[0]!.left).toBe(0); // clamped inside the plot
	});

	it('falls back to the shorter text, then drops the label', () => {
		const r = stackLabels(
			[
				{ x: 50, widths: [80, 30] },
				{ x: 60, widths: [80, 30] },
				{ x: 70, widths: [80, 10] }
			],
			1,
			0,
			400
		);
		expect(r[0]).toEqual({ row: 0, choice: 0, left: 10 });
		expect(r[1]).toBeNull();
		expect(r[2]).toBeNull();
		const two = stackLabels(
			[
				{ x: 50, widths: [80, 30] },
				{ x: 120, widths: [80, 30] }
			],
			1,
			0,
			400
		);
		expect(two[1]).toEqual({ row: 0, choice: 1, left: 105 });
	});
});

describe('projections after CHARGE', () => {
	// A cold charge: BT climbs at 60 °C/min for the first seconds, then settles near 20.
	const t = Array.from({ length: 61 }, (_, i) => i - 10);
	const ror = t.map((s) => (s < 0 ? 0 : s < 20 ? 60 - s : 20));
	const bt = t.map((s) => 25 + Math.max(0, s) * 0.5);
	it('waits until the charge has settled', () => {
		const early = t.filter((s) => s <= 10).length;
		expect(
			projectBt(
				t.slice(0, early),
				bt.slice(0, early),
				ror.slice(0, early),
				120,
				30,
				PROJECT_AFTER_CHARGE_SEC
			)
		).toBeNull();
		expect(
			projectRor(t.slice(0, early), ror.slice(0, early), 120, 30, PROJECT_AFTER_CHARGE_SEC)
		).toBeNull();
	});
	it('fits only samples after the settle time', () => {
		// At 35 s the 30 s window still reaches back into the climb.
		const k = t.filter((s) => s <= 35).length;
		const p = projectRor(t.slice(0, k), ror.slice(0, k), 60, 30, PROJECT_AFTER_CHARGE_SEC)!;
		expect(p.at60).toBeCloseTo(20, 5);
		const loose = projectRor(t.slice(0, k), ror.slice(0, k), 60, 30)!;
		expect(loose.at60).not.toBeCloseTo(20, 1);
	});
});

describe('projectBt with a rising RoR', () => {
	it('stays linear instead of extrapolating the climb', () => {
		const t = Array.from({ length: 31 }, (_, i) => i);
		const ror = t.map((s) => 30 + s); // rising 1 °C/min per second
		const bt = t.map((s) => 70 + s * 0.5);
		const p = projectBt(t, bt, ror, 120)!;
		expect(p.at60).toBeCloseTo(bt[30] + (ror[30] / 60) * 60, 5);
	});
});

describe('declared channels', () => {
	const chans = [
		{ key: 't3', label: 'Exhaust', unit: 'C', max: null },
		{ key: 'heaterApplied', label: 'Applied', unit: '%', max: null },
		{ key: 'gas', label: 'Gas', unit: 'kPa', max: 4 },
		{ key: 'rpm', label: 'Drum', unit: 'rpm', max: null }
	];
	const samples = [
		{ beanTemp: 100, t3: 150, heaterApplied: 40, gas: 2, rpm: 55 },
		{ beanTemp: 101, t3: 160, gas: 'x' },
		null
	];
	it('become series on the right axis, °C converted for display', () => {
		const lines = channelLines(samples, chans, (c) => c * 1.8 + 32);
		expect(lines.map((l) => [l.scale, l.axis])).toEqual([
			['temp', 'temp'],
			['pct', 'pct'],
			['ch:gas', 'own'],
			['ch:rpm', 'own']
		]);
		expect(lines[0].values).toEqual([302, 320, null]);
		expect(lines[1].values).toEqual([40, null, null]);
		expect(lines[2].values).toEqual([2, null, null]);
		expect(lines[2].top).toBe(4);
		expect(lines[3].top).toBe(55);
		expect(lines[0].top).toBeNull();
	});
	it('reads live telemetry as is in °C without a converter', () => {
		const [ex] = channelLines([entry(0, 100, { t3: 150 } as never).telemetry], chans.slice(0, 1));
		expect(ex.values).toEqual([150]);
	});
});
