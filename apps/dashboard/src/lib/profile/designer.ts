/**
 * The profile designer's own logic (docs/profiles.md): the profile's fields, points and events
 * as edited, the phase estimates the chart annotates, and the request a save sends. Points,
 * charge/end temps and the API are always °C; the chart and the inputs work in the display
 * unit and convert at the edge.
 */
import type {
	AutoDropBy,
	CreateProfileRequest,
	ProfileEventInput,
	ProfileExpectations,
	ProfileWithPoints
} from '#lib/api/profiles.js';
import type { RoasterKind } from '#lib/api/roasters.js';
import { planFromCurve, readPlan, type PlanReading } from '#lib/guidance/profile.js';
import { eventTimeOnCurve, toInput } from '#lib/profile/events.js';
import { fromDisplay, roundForInput, tempUnit, toDisplay } from '#lib/stores/units.svelte.js';

export interface DesignerPoint {
	time_seconds: number;
	target_temp: number;
	fan_speed: number | null;
	target_env_temp: number | null;
	heater_pct: number | null;
	notes: string | null;
}

/** The profile's metadata as the form edits it: '' or null is unset. */
export interface DesignerFields {
	name: string;
	description: string;
	beans: string;
	batchSize: number | null;
	roaster: string;
	/** The roaster kind it is for (docs/profiles.md); '' is any. */
	roasterKind: RoasterKind | '';
	/** Roast guidance (docs/roast-guidance.md). */
	roastLevel: string;
	roastPurpose: string;
	expectations: ProfileExpectations | null;
	chargeTemp: number | undefined;
	targetEndTemp: number | undefined;
	/** Auto DROP (docs/replay.md): '' is off. */
	autoDrop: AutoDropBy | '';
}

/** The form's fields from the profile edited, or empty for a new one. */
export function designerFields(initial: ProfileWithPoints | null): DesignerFields {
	return {
		name: initial?.name ?? '',
		description: initial?.description ?? '',
		beans: initial?.beans ?? '',
		batchSize: initial?.batch_size_g ?? null,
		roaster: initial?.roaster ?? '',
		roasterKind: initial?.roaster_kind ?? '',
		roastLevel: initial?.roast_level ?? '',
		roastPurpose: initial?.roast_purpose ?? '',
		expectations: initial?.expectations ?? null,
		chargeTemp: initial?.charge_temp ?? undefined,
		targetEndTemp: initial?.target_end_temp ?? undefined,
		autoDrop: initial?.auto_drop ?? ''
	};
}

/** The profile's points as the designer edits them, in time order. */
export function designerPoints(initial: ProfileWithPoints | null): DesignerPoint[] {
	return initial?.points
		? [...initial.points]
				.sort((a, b) => a.time_seconds - b.time_seconds)
				.map((p) => ({
					time_seconds: p.time_seconds,
					target_temp: p.target_temp,
					fan_speed: p.fan_speed,
					target_env_temp: p.target_env_temp ?? null,
					heater_pct: p.heater_pct ?? null,
					notes: p.notes ?? null
				}))
		: [];
}

/** The profile's events as inputs, the timed ones in order and the BT-triggered ones after. */
export function designerEvents(initial: ProfileWithPoints | null): ProfileEventInput[] {
	return (initial?.events ?? [])
		.map(toInput)
		.sort((a, b) => (a.at_seconds ?? 1e9) - (b.at_seconds ?? 1e9));
}

/** The points by time, a copy. */
export function sortPoints(points: DesignerPoint[]): DesignerPoint[] {
	return [...points].sort((a, b) => a.time_seconds - b.time_seconds);
}

/** A point added by a click: whole seconds, BT to a tenth, nothing else set. */
export function newPoint(timeSec: number, temp: number): DesignerPoint {
	return {
		time_seconds: Math.round(timeSec),
		target_temp: Math.round(temp * 10) / 10,
		fan_speed: null,
		target_env_temp: null,
		heater_pct: null,
		notes: null
	};
}

/** Where a sorted copy's point sits in `points` (the same time and BT), or -1. */
export function indexOfPoint(points: DesignerPoint[], pt: DesignerPoint): number {
	return points.findIndex(
		(p) => p.time_seconds === pt.time_seconds && p.target_temp === pt.target_temp
	);
}

/** Linear BT curve through the points, °C, for placing BT-triggered events. */
export function btCurve(points: DesignerPoint[]): [number, number][] {
	return sortPoints(points).map((p) => [p.time_seconds, p.target_temp]);
}

/** A temperature typed in the display unit, as °C to a hundredth. */
export function parseTemp(v: number): number {
	return roundForInput(fromDisplay(v), 2);
}

/** A °C value for an input, in the display unit; '' for none. */
export function showTemp(c: number | null | undefined): number | '' {
	return c == null ? '' : roundForInput(toDisplay(c));
}

/** A °C value as a label: whole degrees in the display unit, with the unit. */
export function fmtTempLabel(c: number): string {
	return `${roundForInput(toDisplay(c), 0)}${tempUnit()}`;
}

/** Seconds as m:ss. */
export function fmtClock(sec: number): string {
	const s = Math.max(0, Math.round(sec));
	return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, '0')}`;
}

/** m:ss or plain seconds as seconds; null when it is neither. */
export function parseClock(v: string): number | null {
	const m = v.trim().match(/^(\d+):(\d{1,2})$/);
	if (m) return Number(m[1]) * 60 + Number(m[2]);
	const n = Number(v);
	return v.trim() !== '' && Number.isFinite(n) ? n : null;
}

/** Switching action resets the value/label slot to something valid for it. */
export function setAction(e: ProfileEventInput, action: ProfileEventInput['action']) {
	e.action = action;
	if (action === 'fan' || action === 'heater') {
		e.value = e.value != null && e.value <= 100 ? e.value : 50;
	} else if (action === 'setpoint') {
		e.value = e.value != null && e.value > 100 ? e.value : 200;
	} else {
		e.value = null;
		e.label = action === 'mode' ? 'auto' : action === 'landmark' ? 'first_crack_start' : '';
	}
}

/** Switching trigger fills the other reference from the curve when it is empty. */
export function setTrigger(
	e: ProfileEventInput,
	kind: ProfileEventInput['trigger_kind'],
	curve: [number, number][]
) {
	e.trigger_kind = kind;
	if (kind === 'bt' && e.at_bt == null && e.at_seconds != null) {
		const hit = curve.find(([t]) => t >= (e.at_seconds ?? 0));
		e.at_bt = hit ? Math.round(hit[1] * 10) / 10 : 150;
	} else if (kind === 'time' && e.at_seconds == null) {
		e.at_seconds = Math.round(eventTimeOnCurve({ ...e, trigger_kind: 'bt' }, curve) ?? 0);
	}
}

/** Time (seconds) where the interpolated BT crosses a threshold, or null. */
export function findTempCrossing(sorted: DesignerPoint[], threshold: number): number | null {
	if (sorted.length < 2) return null;
	for (let i = 0; i < sorted.length - 1; i++) {
		const p0 = sorted[i];
		const p1 = sorted[i + 1];
		if (p0.target_temp <= threshold && p1.target_temp >= threshold) {
			const frac = (threshold - p0.target_temp) / (p1.target_temp - p0.target_temp);
			return Math.round(p0.time_seconds + frac * (p1.time_seconds - p0.time_seconds));
		}
	}
	return null;
}

/** Phase temperature thresholds, °C: drying's end and first crack when no landmark says. */
export const DRY_TEMP = 150;
export const FC_TEMP = 200;

/** Time of a landmark event on the curve, if the profile has one. */
export function landmarkTime(
	events: ProfileEventInput[],
	curve: [number, number][],
	type: string
): number | null {
	const e = events.find((x) => x.action === 'landmark' && x.label === type);
	return e ? eventTimeOnCurve(e, curve) : null;
}

export interface DtrInfo {
	/** Development time ratio, %, to a tenth. */
	dtr: number;
	fcTime: number;
	totalTime: number;
}

/** The planned DTR: first crack from the FC landmark, else the BT threshold; null without one. */
export function dtrInfo(sorted: DesignerPoint[], events: ProfileEventInput[]): DtrInfo | null {
	if (sorted.length < 2) return null;
	const totalTime = sorted[sorted.length - 1].time_seconds;
	const fcLandmark = landmarkTime(events, btCurve(sorted), 'first_crack_start');
	const fcTime = fcLandmark != null ? Math.round(fcLandmark) : findTempCrossing(sorted, FC_TEMP);
	if (fcTime === null || totalTime <= 0) return null;
	const dtr = ((totalTime - fcTime) / totalTime) * 100;
	return { dtr: Math.round(dtr * 10) / 10, fcTime, totalTime };
}

/** Roast guidance while editing (docs/roast-guidance.md): the planned development against
 * the level, the phase split (information only), and the results the level should give. */
export function planReading(
	dtr: DtrInfo | null,
	sorted: DesignerPoint[],
	events: ProfileEventInput[],
	fields: Pick<DesignerFields, 'roastLevel' | 'expectations'>
): PlanReading | null {
	if (!dtr) return null;
	const fcEvent = events.find((e) => e.action === 'landmark' && e.label === 'first_crack_start');
	const dry =
		landmarkTime(events, btCurve(sorted), 'drying_end') ?? findTempCrossing(sorted, DRY_TEMP);
	const plan = planFromCurve({
		fc_s: dtr.fcTime,
		fc_bt: fcEvent?.at_bt ?? FC_TEMP,
		dry_s: dry,
		total_s: dtr.totalTime
	});
	return readPlan(plan, {
		roast_level: fields.roastLevel || null,
		expectations: fields.expectations
	});
}

/** An axis range that always gives the chart a valid mapping, even with zero or few points. */
export function axisRange(
	vals: number[],
	defaultMax: number,
	padding: number
): { min: number; max: number } {
	if (vals.length === 0) return { min: 0, max: defaultMax };
	const lo = Math.min(...vals);
	const hi = Math.max(...vals);
	return { min: Math.max(0, Math.floor(lo - padding)), max: Math.ceil(hi + padding) };
}

/** Why the profile cannot be saved yet, or null. */
export function saveProblem(o: {
	fields: DesignerFields;
	points: DesignerPoint[];
	batchBad: boolean;
	eventErrors: (string | null)[];
	weightUnit: string;
}): string | null {
	if (!o.fields.name.trim()) return 'Profile name is required';
	if (o.points.length === 0) return 'At least one point is required';
	if (o.batchBad)
		return `Batch size is not a weight: a number of ${o.weightUnit}, or 100 g, 3.5 oz`;
	const bad = o.eventErrors.findIndex((e) => e !== null);
	if (bad >= 0) return `Event ${bad + 1}: ${o.eventErrors[bad]}`;
	return null;
}

/** The request a save sends: the fields, the points in order, the events; first crack from a
 * timed FC landmark, else the stored target (the server replaces every field, so leaving it
 * out cleared it); the curves kept unless removed (a new profile has none to send). */
export function profileRequest(o: {
	fields: DesignerFields;
	points: DesignerPoint[];
	events: ProfileEventInput[];
	initial: ProfileWithPoints | null;
	removeCurves: boolean;
}): CreateProfileRequest {
	const { fields: f, initial } = o;
	const sorted = sortPoints(o.points);
	const totalTime = sorted.length > 0 ? sorted[sorted.length - 1].time_seconds : undefined;
	const fcEvent = o.events.find(
		(e) =>
			e.action === 'landmark' &&
			e.label === 'first_crack_start' &&
			e.trigger_kind === 'time' &&
			e.at_seconds != null
	);
	const req: CreateProfileRequest = {
		name: f.name.trim(),
		description: f.description.trim() || undefined,
		beans: f.beans.trim() || undefined,
		batch_size_g: f.batchSize && f.batchSize > 0 ? f.batchSize : undefined,
		roaster: f.roaster.trim() || undefined,
		roaster_kind: f.roasterKind || null,
		roast_level: f.roastLevel || null,
		roast_purpose: f.roastPurpose || null,
		expectations: f.expectations,
		target_first_crack:
			fcEvent?.at_seconds != null
				? Math.round(fcEvent.at_seconds)
				: (initial?.target_first_crack ?? undefined),
		preheat_temp: initial?.preheat_temp ?? undefined,
		charge_temp: f.chargeTemp,
		target_end_temp: f.targetEndTemp,
		target_total_time: totalTime,
		auto_drop: f.autoDrop || null,
		points: sorted.map((p) => ({
			time_seconds: p.time_seconds,
			target_temp: p.target_temp,
			fan_speed: p.fan_speed ?? undefined,
			target_env_temp: p.target_env_temp ?? undefined,
			heater_pct: p.heater_pct ?? undefined,
			notes: p.notes ?? undefined
		})),
		events: o.events.map(toInput)
	};
	if (initial?.id && o.removeCurves) req.curves = null;
	return req;
}
