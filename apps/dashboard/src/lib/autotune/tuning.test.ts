import { describe, it, expect } from 'vitest';
import {
	scheduleGainsAt,
	warningText,
	qualityInfo,
	errorText,
	fmtGains,
	scheduleProblem,
	cleanRows,
	tauCLabel,
	msInfo,
	toDeviceGains,
	describeModel,
	tunePhaseInfo
} from './tuning.js';

describe('tuning helpers', () => {
	const sched = {
		enabled: true,
		rows: [
			{ fan_pct: 100, kp: 10, ki: 0.2, kd: 0 },
			{ fan_pct: 75, kp: 6, ki: 0.1, kd: 0 }
		]
	};

	it('interpolates the schedule like the server', () => {
		expect(scheduleGainsAt(sched, 100)?.kp).toBe(10);
		expect(scheduleGainsAt(sched, 120)?.kp).toBe(10);
		expect(scheduleGainsAt(sched, 40)?.kp).toBe(6);
		const mid = scheduleGainsAt(sched, 87.5)!;
		expect(mid.kp).toBeCloseTo(8);
		expect(mid.ki).toBeCloseTo(0.15);
		expect(scheduleGainsAt({ ...sched, enabled: false }, 90)).toBeNull();
		expect(scheduleGainsAt({ enabled: true, rows: [] }, 90)).toBeNull();
		expect(scheduleGainsAt(sched, null)).toBeNull();
	});

	it('explains warnings and grades runs', () => {
		expect(warningText('retargeted')).toMatch(/could not reach/);
		expect(warningText('new_thing')).toBe('new thing');
		expect(qualityInfo('good').tone).toBe('good');
		expect(qualityInfo('acceptable', 'complete').label).toBe('Acceptable');
		expect(qualityInfo('poor', 'failed').label).toBe('Failed');
		expect(qualityInfo(null).label).toBe('Unrated');
	});

	it('formats errors and gains', () => {
		expect(errorText(new Error('API error 409: cannot apply: the run failed'))).toBe(
			'cannot apply: the run failed'
		);
		expect(fmtGains({ kp: 6.25, ki: 0.04734, kd: 0 })).toEqual({
			kp: '6.25',
			ki: '0.0473',
			kd: '0.00'
		});
		expect(fmtGains(null).kp).toBe('—');
	});

	it('an emptied Kd box saves as 0; an empty Kp or Ki stops the save', () => {
		const empty = null as unknown as number;
		const rows = [
			{ fan_pct: 100, kp: 7.29, ki: 0.091, kd: empty },
			{ fan_pct: 71, kp: 11.55, ki: 0.181, kd: 0 }
		];
		expect(scheduleProblem(rows)).toBeNull();
		expect(cleanRows(rows).map((r) => [r.fan_pct, r.kd])).toEqual([
			[71, 0],
			[100, 0]
		]);
		expect(scheduleProblem([{ ...rows[0], kp: empty }])).toBe('Kp is empty at 100 % fan.');
		expect(scheduleProblem([{ ...rows[0], ki: '' as unknown as number }])).toContain('Ki is empty');
	});

	it('autotune 2 labels', () => {
		expect(tauCLabel(1)).toBe('tight');
		expect(tauCLabel(1.5)).toBe('balanced');
		expect(tauCLabel(3)).toBe('smooth');
		expect(msInfo(1.4).tone).toBe('good');
		expect(msInfo(1.9).tone).toBe('warn');
		expect(msInfo(2.6).label).toBe('fragile');
		expect(toDeviceGains({ kp: 2, ki: 0.04 })).toEqual({ kp: 5.1, ki: 0.102, kd: 0 });
		expect(describeModel({ k: 0.006, t1: 0, t2: 0, l: 7.5, integrating: true })).toBe(
			'an integrator: 0.36 °C/min per % heater, 7.5 s dead time'
		);
		expect(tunePhaseInfo('symmetric').label).toBe('Finding the holding power');
	});
});
