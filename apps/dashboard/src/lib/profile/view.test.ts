import { describe, it, expect } from 'vitest';
import type { ProfileWithPoints } from '#lib/api/profiles.js';
import { profileView, valueAt, controlLabel } from './view.js';

function profile(over: Partial<ProfileWithPoints> = {}): ProfileWithPoints {
	const pt = (
		time_seconds: number,
		target_temp: number,
		fan_speed: number,
		heater_pct: number | null,
		target_env_temp: number | null = null
	) => ({
		id: `p${time_seconds}`,
		profile_id: 'x',
		time_seconds,
		target_temp,
		fan_speed,
		heater_pct,
		target_env_temp,
		notes: null,
		created_at: ''
	});
	return {
		id: 'x',
		name: 'Light espresso',
		description: null,
		target_total_time: 600,
		target_first_crack: 480,
		target_end_temp: 212,
		charge_temp: 30,
		created_at: '',
		points: [
			pt(0, 30, 100, 100, 240),
			pt(240, 160, 100, 90, 250),
			pt(480, 200, 85, 80, 255),
			pt(600, 212, 80, 70, 255)
		],
		events: [
			{
				id: 'd',
				profile_id: 'x',
				trigger_kind: 'bt',
				at_seconds: null,
				at_bt: 160,
				action: 'landmark',
				value: null,
				label: 'drying_end',
				created_at: ''
			},
			{
				id: 's',
				profile_id: 'x',
				trigger_kind: 'time',
				at_seconds: 300,
				at_bt: null,
				action: 'setpoint',
				value: 185,
				label: null,
				created_at: ''
			}
		],
		...over
	} as ProfileWithPoints;
}

describe('profileView', () => {
	it('places landmarks from events, targets and the curve', () => {
		const v = profileView(profile());
		expect(
			v.landmarks.map((l) => [
				l.code,
				Math.round(l.t),
				l.bt != null ? Math.round(l.bt) : null,
				l.source
			])
		).toEqual([
			['CHARGE', 0, 30, 'curve'],
			['DRY', 240, 160, 'event'],
			['FCs', 480, 200, 'target'],
			['DROP', 600, 212, 'target']
		]);
		expect(v.total).toBe(600);
		expect(v.dtr).toBeCloseTo(20, 5);
	});

	it('has every curve Artisan shows for a background', () => {
		const v = profileView(profile());
		expect(v.t.length).toBe(601);
		expect(v.hasEt && v.hasFan && v.hasHeater).toBe(true);
		// Through the points, on a spline: RoR declines with no step at a point.
		expect(valueAt(v.t, v.bt, 240)).toBeCloseTo(160, 6);
		const ror = (t: number) => valueAt(v.t, v.btRor, t) as number;
		expect(ror(60)).toBeGreaterThan(ror(300));
		expect(ror(300)).toBeGreaterThan(ror(540));
		expect(Math.abs(ror(235) - ror(245))).toBeLessThan(3); // straight segments step by ~22 here
		expect(valueAt(v.t, v.etRor, 120)).toBeGreaterThan(0);
		// Fan and heater hold from point to point.
		expect(valueAt(v.t, v.fan, 500)).toBe(85);
		expect(valueAt(v.t, v.heater, 300)).toBe(90);
	});

	it('computes phases like the roast report', () => {
		const v = profileView(profile());
		expect(
			v.phases.map((p) => [p.key, p.duration, Math.round(p.pct ?? 0), Math.round(p.dT ?? 0)])
		).toEqual([
			['drying', 240, 40, 130],
			['maillard', 240, 40, 40],
			['development', 120, 20, 12]
		]);
	});

	it('lists control events in time, landmarks apart', () => {
		const v = profileView(profile());
		expect(v.controls.map((c) => [c.action, c.t])).toEqual([['setpoint', 300]]);
		expect(controlLabel(v.controls[0], (c) => `${c}°C`)).toBe('Setpoint 185°C');
	});

	it('finds the turning point of a hot charge', () => {
		const v = profileView(
			profile({
				events: [],
				points: [
					{ time_seconds: 0, target_temp: 180 },
					{ time_seconds: 60, target_temp: 90 },
					{ time_seconds: 300, target_temp: 160 },
					{ time_seconds: 600, target_temp: 210 }
				] as ProfileWithPoints['points']
			})
		);
		const tp = v.landmarks.find((l) => l.code === 'TP');
		expect(tp?.t).toBe(60);
		expect(tp?.bt).toBe(90);
		// Drying ΔT runs from TP, as Artisan's does; no DRY landmark here, so no drying row.
		expect(v.phases.map((p) => p.key)).toEqual(['development']);
	});

	it('falls back to the recorded curves when a profile has no points', () => {
		const v = profileView(
			profile({
				points: [],
				curves: {
					source_session_id: null,
					interval_seconds: 2,
					bt: [30, 40, 50, 60],
					et: [200, 200, 200, 200],
					ror: [],
					heater: [],
					fan: []
				}
			})
		);
		expect(v.t).toEqual([0, 2, 4, 6]);
		expect(v.recorded).toBeNull();
		expect(v.landmarks.at(-1)?.code).toBe('DROP');
	});
});
