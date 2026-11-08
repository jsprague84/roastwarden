import { describe, it, expect } from 'vitest';
import { fromAlrm, toAlrm, isAlrm, type AlrmFile } from './artisan.js';
import type { AlarmRule } from './alarms.js';

/** An Artisan file from row objects (columns as Artisan writes them). */
function alrm(
	rows: Partial<{
		flag: number;
		guard: number;
		neg: number;
		time: number;
		offset: number;
		cond: number;
		source: number;
		temp: number;
		action: number;
		beep: number;
		str: string;
	}>[]
): AlrmFile {
	const d = {
		flag: 1,
		guard: -1,
		neg: -1,
		time: -1,
		offset: 0,
		cond: 1,
		source: 1,
		temp: 0,
		action: 0,
		beep: 1,
		str: ''
	};
	const r = rows.map((x) => ({ ...d, ...x }));
	return {
		alarmflags: r.map((x) => x.flag),
		alarmguards: r.map((x) => x.guard),
		alarmnegguards: r.map((x) => x.neg),
		alarmtimes: r.map((x) => x.time),
		alarmoffsets: r.map((x) => x.offset),
		alarmconds: r.map((x) => x.cond),
		alarmsources: r.map((x) => x.source),
		alarmtemperatures: r.map((x) => x.temp),
		alarmactions: r.map((x) => x.action),
		alarmbeep: r.map((x) => x.beep),
		alarmstrings: r.map((x) => x.str)
	};
}

describe('Artisan .alrm import', () => {
	it('maps temperature, RoR and time rows', () => {
		const { rules, notes } = fromAlrm(
			alrm([
				{ source: 1, cond: 1, temp: 205, time: 0, action: 0, str: 'Nearly there' },
				{ source: 0, cond: 0, temp: 150, time: 8, action: 3, str: '80' },
				{ source: -1, cond: 0, temp: 5, time: 2, action: 6, str: '40' },
				{ source: -3, time: 6, offset: 30, action: 3, str: '100' }
			]),
			'C'
		);
		expect(notes).toEqual([]);
		expect(rules).toEqual([
			{
				name: 'Nearly there',
				condition_type: 'temp_above',
				threshold: 205,
				enabled: true,
				armed_after: 'charge'
			},
			{
				name: 'Artisan 2: ET < 150',
				condition_type: 'et_below',
				threshold: 150,
				enabled: true,
				armed_after: 'turning_point',
				actions: [{ type: 'fan', value: 80 }]
			},
			{
				name: 'Artisan 3: RoR < 5',
				condition_type: 'ror_below',
				threshold: 5,
				enabled: true,
				armed_after: 'first_crack_start',
				actions: [{ type: 'heater', value: 40 }]
			},
			{
				name: 'Artisan 4: 0:30 after DROP',
				condition_type: 'time_after_event',
				threshold: 30,
				enabled: true,
				reference_event: 'drop',
				actions: [{ type: 'fan', value: 100 }]
			}
		]);
	});

	it('converts Fahrenheit files', () => {
		const { rules } = fromAlrm(
			alrm([
				{ source: 1, temp: 410, action: 21, str: '400' },
				{ source: -1, cond: 0, temp: 9 }
			]),
			'F'
		);
		expect(rules[0].threshold).toBeCloseTo(210, 1);
		expect(rules[0].actions).toEqual([{ type: 'setpoint', value: 204.4 }]);
		expect(rules[1].threshold).toBe(5);
	});

	it('folds an If Alarm chain into one rule with several actions', () => {
		const { rules, notes } = fromAlrm(
			alrm([
				{ source: 1, temp: 230, action: 6, str: '0' },
				{ time: 10, guard: 0, source: -3, action: 3, str: '100' },
				{ time: 10, guard: 1, source: -3, action: 13 }
			]),
			'C'
		);
		expect(notes).toEqual([]);
		expect(rules).toHaveLength(1);
		expect(rules[0].actions).toEqual([
			{ type: 'heater', value: 0 },
			{ type: 'fan', value: 100 },
			{ type: 'mark', event: 'drop' }
		]);
	});

	it('turns guards into requires by name', () => {
		const { rules } = fromAlrm(
			alrm([
				{ temp: 180, action: 0, str: 'hot' },
				{ temp: 190, guard: 0, action: 20 }
			]),
			'C'
		);
		expect(rules[1].requires).toBe('hot');
		expect(rules[1].actions).toEqual([{ type: 'mode', mode: 'manual' }]);
	});

	it('reports rows that do not map instead of guessing', () => {
		const { rules, notes } = fromAlrm(
			alrm([
				{ source: -2, temp: 10 },
				{ source: 1, cond: 2, temp: 200 },
				{ source: 4, temp: 100 },
				{ source: 1, temp: 200, offset: 60, action: 1, str: 'beep.sh' },
				{ source: 1, temp: 210, guard: 0 },
				{ source: -3, time: 10, guard: 3, offset: 20 },
				{ source: 1, temp: 220, neg: 3, flag: 0 }
			]),
			'C'
		);
		expect(rules.map((r) => r.name)).toEqual(['Artisan 4: BT > 200', 'Artisan 7: BT > 220']);
		expect(rules[1].enabled).toBe(false);
		expect(notes.map((n) => `${n.at} ${n.kind}`)).toEqual([
			'row 1 skipped',
			'row 2 skipped',
			'row 3 skipped',
			'row 4 partial',
			'row 4 partial',
			'row 5 skipped',
			'row 6 skipped',
			'row 7 partial'
		]);
	});

	it('recognises an alarm file', () => {
		expect(isAlrm(alrm([{}]))).toBe(true);
		expect(isAlrm([{ name: 'x' }])).toBe(false);
	});
});

describe('Artisan .alrm export', () => {
	const rules: AlarmRule[] = [
		{
			name: 'Too hot',
			condition_type: 'temp_above',
			threshold: 235,
			enabled: true,
			actions: [{ type: 'heater_off' }, { type: 'fan', value: 100 }]
		},
		{
			name: 'After drop',
			condition_type: 'time_after_event',
			threshold: 0,
			reference_event: 'drop',
			enabled: true,
			actions: [{ type: 'replay_stop' }]
		},
		{ name: 'Behind', condition_type: 'delta_below', threshold: -8, enabled: true },
		{ name: 'Lost', condition_type: 'no_telemetry', threshold: 15, enabled: true },
		{
			name: 'Slow',
			condition_type: 'ror_below',
			threshold: 5,
			enabled: false,
			armed_after: 'first_crack_start',
			repeat: true
		},
		{
			name: 'Then',
			condition_type: 'temp_above',
			threshold: 240,
			enabled: true,
			requires: 'Too hot',
			actions: [{ type: 'emergency_stop' }]
		}
	];

	it('writes one row per action, chained with If Alarm', () => {
		const { file, notes } = toAlrm(rules, 'C');
		expect(file.alarmactions).toEqual([6, 3, 23, 0, 0]);
		expect(file.alarmstrings).toEqual(['0', '100', '', 'Slow', 'Then']);
		expect(file.alarmguards).toEqual([-1, 0, -1, -1, 0]);
		expect(file.alarmtimes).toEqual([-1, 10, 6, 2, -1]);
		expect(file.alarmsources).toEqual([1, -3, -3, -1, 1]);
		expect(file.alarmflags).toEqual([1, 1, 1, 0, 1]);
		expect(file.alarmnegguards).toEqual([-1, -1, -1, -1, -1]);
		expect(notes.map((n) => `${n.at} ${n.kind}`)).toEqual([
			'Behind skipped',
			'Lost skipped',
			'Slow partial',
			'Then partial'
		]);
	});

	it('writes Fahrenheit when asked', () => {
		const { file } = toAlrm([rules[0], rules[4]], 'F');
		expect(file.alarmtemperatures).toEqual([455, 0, 9]);
	});

	it('round-trips what both sides can express', () => {
		const keep = rules.filter(
			(r) => !['delta_below', 'no_telemetry'].includes(r.condition_type) && r.name !== 'Then'
		);
		const back = fromAlrm(toAlrm(keep, 'C').file, 'C').rules;
		expect(back.map((r) => [r.condition_type, r.threshold, r.enabled, r.actions ?? []])).toEqual(
			keep.map((r) => [
				r.condition_type,
				r.threshold,
				r.enabled,
				(r.actions ?? []).map((a) => (a.type === 'heater_off' ? { type: 'heater', value: 0 } : a))
			])
		);
	});
});
