import { describe, expect, it } from 'vitest';
import type { ProfileEventInput, ProfileWithPoints } from '#lib/api/profiles.js';
import { blankEvent } from '#lib/profile/events.js';
import {
	axisRange,
	btCurve,
	designerEvents,
	designerFields,
	designerPoints,
	dtrInfo,
	findTempCrossing,
	fmtClock,
	indexOfPoint,
	landmarkTime,
	newPoint,
	parseClock,
	planReading,
	profileRequest,
	saveProblem,
	setAction,
	setTrigger,
	sortPoints,
	type DesignerPoint
} from './designer.js';

const pt = (
	time_seconds: number,
	target_temp: number,
	extra: Partial<DesignerPoint> = {}
): DesignerPoint => ({
	time_seconds,
	target_temp,
	fan_speed: null,
	target_env_temp: null,
	heater_pct: null,
	notes: null,
	...extra
});

/** A light roast: charge, turning point, drying's end, first crack, drop at 10:00. */
const CURVE = [pt(0, 180), pt(60, 95), pt(240, 150), pt(480, 200), pt(600, 215)];

const profile: ProfileWithPoints = {
	id: 'p1',
	name: 'Light espresso',
	description: 'desc',
	target_total_time: 600,
	target_first_crack: 470,
	target_end_temp: 215,
	preheat_temp: 190,
	charge_temp: 180,
	beans: 'Guji',
	batch_size_g: 250,
	roaster: 'Fluid bed',
	roast_level: 'light',
	roast_purpose: 'espresso',
	expectations: { weight_loss_pct: [12, 14] },
	auto_drop: 'development',
	roaster_kind: 'fluid_bed_electric',
	created_at: '2026-01-01T00:00:00Z',
	points: [
		{
			id: 'b',
			time_seconds: 600,
			target_temp: 215,
			fan_speed: null,
			notes: null,
			target_env_temp: null
		},
		{
			id: 'a',
			time_seconds: 0,
			target_temp: 180,
			fan_speed: 70,
			notes: 'in',
			target_env_temp: 200,
			heater_pct: 80
		}
	],
	events: [
		{
			id: 'e2',
			profile_id: 'p1',
			created_at: '',
			trigger_kind: 'bt',
			at_seconds: null,
			at_bt: 196,
			action: 'landmark',
			value: null,
			label: 'first_crack_start'
		},
		{
			id: 'e1',
			profile_id: 'p1',
			created_at: '',
			trigger_kind: 'time',
			at_seconds: 30,
			at_bt: null,
			action: 'fan',
			value: 80,
			label: null
		}
	]
};

describe('the form, the points and the events from a profile', () => {
	it('reads the fields, with empty strings for what is unset', () => {
		expect(designerFields(profile)).toEqual({
			name: 'Light espresso',
			description: 'desc',
			beans: 'Guji',
			batchSize: 250,
			roaster: 'Fluid bed',
			roasterKind: 'fluid_bed_electric',
			roastLevel: 'light',
			roastPurpose: 'espresso',
			expectations: { weight_loss_pct: [12, 14] },
			chargeTemp: 180,
			targetEndTemp: 215,
			autoDrop: 'development'
		});
		const blank = designerFields(null);
		expect(blank.name).toBe('');
		expect(blank.batchSize).toBeNull();
		expect(blank.chargeTemp).toBeUndefined();
		expect(blank.autoDrop).toBe('');
	});

	it('sorts the points by time and fills the optional columns with null', () => {
		const points = designerPoints(profile);
		expect(points.map((p) => p.time_seconds)).toEqual([0, 600]);
		expect(points[0]).toEqual(
			pt(0, 180, { fan_speed: 70, target_env_temp: 200, heater_pct: 80, notes: 'in' })
		);
		expect(points[1]).toEqual(pt(600, 215));
		expect(designerPoints(null)).toEqual([]);
	});

	it('puts timed events first and BT-triggered ones after', () => {
		const events = designerEvents(profile);
		expect(events.map((e) => e.action)).toEqual(['fan', 'landmark']);
		expect(events[0]).not.toHaveProperty('id');
	});
});

describe('points', () => {
	it('sorts a copy and finds a sorted point in the original', () => {
		const points = [pt(60, 95), pt(0, 180)];
		const sorted = sortPoints(points);
		expect(sorted.map((p) => p.time_seconds)).toEqual([0, 60]);
		expect(points[0].time_seconds).toBe(60);
		expect(indexOfPoint(points, sorted[0])).toBe(1);
		expect(indexOfPoint(points, pt(5, 5))).toBe(-1);
	});

	it('makes a clicked point with whole seconds and BT to a tenth', () => {
		expect(newPoint(12.6, 150.26)).toEqual(pt(13, 150.3));
	});

	it('draws the BT curve through the points in order', () => {
		expect(btCurve([pt(60, 95), pt(0, 180)])).toEqual([
			[0, 180],
			[60, 95]
		]);
	});
});

describe('clock text', () => {
	it('formats and parses m:ss, and parses plain seconds', () => {
		expect(fmtClock(0)).toBe('0:00');
		expect(fmtClock(65.4)).toBe('1:05');
		expect(fmtClock(-3)).toBe('0:00');
		expect(parseClock('1:05')).toBe(65);
		expect(parseClock(' 90 ')).toBe(90);
		expect(parseClock('')).toBeNull();
		expect(parseClock('1:5x')).toBeNull();
	});
});

describe('an event edited', () => {
	it('gives a changed action a value that fits it', () => {
		const e: ProfileEventInput = blankEvent(0);
		setAction(e, 'heater');
		expect(e.value).toBe(50);
		e.value = 250;
		setAction(e, 'fan');
		expect(e.value).toBe(50);
		setAction(e, 'setpoint');
		expect(e.value).toBe(200);
		setAction(e, 'mode');
		expect(e.value).toBeNull();
		expect(e.label).toBe('auto');
		setAction(e, 'landmark');
		expect(e.label).toBe('first_crack_start');
		setAction(e, 'note');
		expect(e.label).toBe('');
	});

	it('fills the other trigger from the curve when it switches', () => {
		const curve = btCurve(CURVE);
		const e: ProfileEventInput = { ...blankEvent(300), action: 'fan', value: 60 };
		setTrigger(e, 'bt', curve);
		expect(e.trigger_kind).toBe('bt');
		expect(e.at_bt).toBe(200); // the first point at or after 300 s
		const back: ProfileEventInput = { ...e, at_seconds: null };
		setTrigger(back, 'time', curve);
		expect(back.at_seconds).toBe(480); // BT 200 °C is the point at 8:00
		const late: ProfileEventInput = { ...blankEvent(900), action: 'fan', value: 60 };
		setTrigger(late, 'bt', curve);
		expect(late.at_bt).toBe(150); // past the curve: the fallback
	});
});

describe('phases from the curve', () => {
	it('finds where BT crosses a threshold, interpolated', () => {
		expect(findTempCrossing(CURVE, 150)).toBe(240);
		expect(findTempCrossing(CURVE, 175)).toBe(360);
		expect(findTempCrossing(CURVE, 300)).toBeNull();
		expect(findTempCrossing([pt(0, 100)], 50)).toBeNull();
	});

	it('reads a landmark event off the curve', () => {
		const fc: ProfileEventInput = {
			...blankEvent(0),
			trigger_kind: 'bt',
			at_seconds: null,
			at_bt: 196,
			action: 'landmark',
			label: 'first_crack_start'
		};
		expect(landmarkTime([fc], btCurve(CURVE), 'first_crack_start')).toBeCloseTo(460.8, 0);
		expect(landmarkTime([fc], btCurve(CURVE), 'drying_end')).toBeNull();
	});

	it('plans the DTR from the FC landmark, else from 200 °C', () => {
		expect(dtrInfo(CURVE, [])).toEqual({ dtr: 20, fcTime: 480, totalTime: 600 });
		const fc: ProfileEventInput = {
			...blankEvent(450),
			action: 'landmark',
			label: 'first_crack_start'
		};
		expect(dtrInfo(CURVE, [fc])).toEqual({ dtr: 25, fcTime: 450, totalTime: 600 });
		expect(dtrInfo([pt(0, 100)], [])).toBeNull();
		expect(dtrInfo([pt(0, 100), pt(600, 150)], [])).toBeNull();
	});

	it('reads the plan against the level only with a DTR', () => {
		expect(planReading(null, CURVE, [], { roastLevel: 'light', expectations: null })).toBeNull();
		const read = planReading(dtrInfo(CURVE, []), CURVE, [], {
			roastLevel: 'light',
			expectations: null
		});
		expect(read?.development_s).toBe(120);
		expect(read?.phases).toContain('%');
	});
});

describe('the chart axes', () => {
	it('pads the range to whole numbers and never below zero', () => {
		expect(axisRange([], 720, 30)).toEqual({ min: 0, max: 720 });
		expect(axisRange([10, 500], 720, 30)).toEqual({ min: 0, max: 530 });
		expect(axisRange([100.5, 220.2], 250, 20)).toEqual({ min: 80, max: 241 });
	});
});

describe('a save', () => {
	const fields = designerFields(profile);
	it('says what stops it', () => {
		const ok = { fields, points: CURVE, batchBad: false, eventErrors: [null], weightUnit: 'g' };
		expect(saveProblem(ok)).toBeNull();
		expect(saveProblem({ ...ok, fields: { ...fields, name: ' ' } })).toBe(
			'Profile name is required'
		);
		expect(saveProblem({ ...ok, points: [] })).toBe('At least one point is required');
		expect(saveProblem({ ...ok, batchBad: true })).toContain('a number of g');
		expect(saveProblem({ ...ok, eventErrors: [null, 'needs a value'] })).toBe(
			'Event 2: needs a value'
		);
	});

	it('sends the fields trimmed, the points in order and first crack from a timed landmark', () => {
		const fc: ProfileEventInput = {
			...blankEvent(470.4),
			action: 'landmark',
			label: 'first_crack_start'
		};
		const req = profileRequest({
			fields: { ...fields, description: '  ', roaster: ' Fluid bed ' },
			points: [pt(600, 215, { fan_speed: 60 }), pt(0, 180, { notes: 'in' })],
			events: [fc],
			initial: profile,
			removeCurves: true
		});
		expect(req.name).toBe('Light espresso');
		expect(req.description).toBeUndefined();
		expect(req.roaster).toBe('Fluid bed');
		expect(req.target_first_crack).toBe(470);
		expect(req.target_total_time).toBe(600);
		expect(req.preheat_temp).toBe(190);
		expect(req.points.map((p) => p.time_seconds)).toEqual([0, 600]);
		expect(req.points[0].notes).toBe('in');
		expect(req.points[0].fan_speed).toBeUndefined();
		expect(req.events).toHaveLength(1);
		expect(req.curves).toBeNull();
	});

	it('keeps the stored first crack without a timed landmark, and never sends curves for a new profile', () => {
		const req = profileRequest({
			fields: { ...designerFields(null), name: 'New', batchSize: 0 },
			points: [pt(0, 180)],
			events: [],
			initial: null,
			removeCurves: true
		});
		expect(req.target_first_crack).toBeUndefined();
		expect(req.batch_size_g).toBeUndefined();
		expect(req.roaster_kind).toBeNull();
		expect(req.auto_drop).toBeNull();
		expect('curves' in req).toBe(false);
		const kept = profileRequest({
			fields,
			points: [pt(0, 180)],
			events: [],
			initial: profile,
			removeCurves: false
		});
		expect(kept.target_first_crack).toBe(470);
		expect('curves' in kept).toBe(false);
	});
});
