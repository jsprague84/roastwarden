import { describe, expect, it } from 'vitest';
import type { RoastEvent } from '#lib/api/events.js';
import type { SessionTelemetryPoint } from '#lib/api/sessions.js';
import { roastSummary } from './report.js';
import {
	aucBetween,
	aucConfig,
	consistencyBand,
	fcDropRorSlope,
	liveAuc,
	meanSd,
	phaseAuc,
	slope
} from './stats.js';

const ev = (
	event_type: string,
	elapsed_seconds: number,
	temperature: number | null = null
): RoastEvent =>
	({
		id: event_type,
		session_id: 's',
		event_type,
		elapsed_seconds,
		temperature,
		notes: null,
		created_at: ''
	}) as RoastEvent;

/** 1 Hz telemetry from `bt(t)`, session time = roast time + 10 (CHARGE at 10 s). */
function tele(bt: (t: number) => number, until: number): SessionTelemetryPoint[] {
	const out: SessionTelemetryPoint[] = [];
	for (let t = 0; t <= until + 10; t++)
		out.push({ elapsed_seconds: t, bean_temp: bt(t - 10) } as SessionTelemetryPoint);
	return out;
}

describe('AUC', () => {
	it('integrates BT above the base in °C·min', () => {
		const flat: [number, number][] = [
			[0, 100],
			[60, 100],
			[120, 100]
		];
		expect(aucBetween(flat, 0, 120, 0)).toBeCloseTo(200);
		expect(aucBetween(flat, 0, 120, 50)).toBeCloseTo(100);
		// Ends inside the samples are interpolated; outside them nothing counts.
		expect(aucBetween(flat, 30, 90, 0)).toBeCloseTo(100);
		expect(aucBetween(flat, -60, 0, 0)).toBe(0);
		// A ramp 0 → 120 °C over 2 min, base 60: a triangle of 60 °C × 1 min / 2.
		expect(
			aucBetween(
				[
					[0, 0],
					[120, 120]
				],
				0,
				120,
				60
			)
		).toBeCloseTo(30);
	});

	it('splits per phase and sums to the total from CHARGE', () => {
		const events = [
			ev('charge', 10),
			ev('drying_end', 250),
			ev('first_crack_start', 490),
			ev('drop', 610)
		];
		const t = tele(() => 150, 700);
		const sum = roastSummary(events, t);
		const a = phaseAuc(sum, t);
		expect(a.drying).toBeCloseTo(600); // 4 min × 150
		expect(a.maillard).toBeCloseTo(600);
		expect(a.development).toBeCloseTo(300);
		expect(a.total).toBeCloseTo(1500); // stops at DROP: cooling does not count
		// From first crack only the development counts.
		const fcOnly = phaseAuc(sum, t, { base: 0, start: 'first_crack_start' });
		expect(fcOnly.drying).toBeNull();
		expect(fcOnly.total).toBeCloseTo(300);
	});

	it('reads the settings', () => {
		expect(aucConfig({ auc_base_temp: '100', auc_start_event: 'turning_point' })).toEqual({
			base: 100,
			start: 'turning_point'
		});
		expect(aucConfig({ auc_start_event: 'nonsense' })).toEqual({ base: 0, start: 'charge' });
		expect(aucConfig(null)).toEqual({ base: 0, start: 'charge' });
	});

	it('compares live AUC with the profile', () => {
		const roastSoFar: [number, number][] = [
			[0, 100],
			[60, 100]
		];
		const profile: [number, number][] = [
			[0, 110],
			[120, 110]
		];
		const l = liveAuc(roastSoFar, profile, { base: 0, start: 'charge' })!;
		expect(l.now).toBeCloseTo(100);
		expect(l.profileNow).toBeCloseTo(110);
		expect(l.target).toBeCloseTo(220);
		expect(liveAuc(roastSoFar, null, { base: 0, start: 'charge' })).toEqual({
			now: 100,
			profileNow: null,
			target: null
		});
	});
});

describe('RoR slope and bands', () => {
	it('fits a slope', () => {
		expect(
			slope([
				[0, 1],
				[1, 3],
				[2, 5]
			])
		).toBeCloseTo(2);
		expect(slope([[0, 1]])).toBeNull();
	});

	it('measures how RoR falls from first crack to drop', () => {
		// BT = 150 + 20 t − 1 t² (minutes): RoR = 20 − 2 t °C/min, falling 2 °C/min per minute.
		const bt = (s: number) => {
			const m = s / 60;
			return 150 + 20 * m - m * m;
		};
		const events = [ev('charge', 10), ev('first_crack_start', 250), ev('drop', 490)];
		const t = tele(bt, 600);
		expect(fcDropRorSlope(roastSummary(events, t), t)).toBeCloseTo(-2, 1);
		// No DROP: no slope.
		expect(fcDropRorSlope(roastSummary(events.slice(0, 2), t), t)).toBeNull();
	});

	it('bands several curves where two or more have values', () => {
		const a: [number, number][] = [
			[0, 100],
			[10, 110]
		];
		const b: [number, number][] = [
			[0, 102],
			[20, 122]
		];
		const band = consistencyBand([a, b], 0, 20, 10);
		expect(band.map((p) => p.t)).toEqual([0, 10]);
		expect(band[0].mean).toBeCloseTo(101);
		expect(band[0].sd).toBeCloseTo(Math.SQRT2);
		expect(meanSd([1, 3, null])).toEqual({ mean: 2, sd: Math.SQRT2, n: 2 });
		expect(meanSd([1])).toBeNull();
	});
});
