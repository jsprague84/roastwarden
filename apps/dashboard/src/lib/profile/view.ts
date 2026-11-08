/**
 * What a profile looks like as a roast (the Profiles page, docs/profiles.md):
 * the curves Artisan shows for a background profile (BT, ET, both RoRs, fan
 * and heater), its landmarks with time and BT, the phases, and its control
 * events placed in time. Built with the scope's own helpers, so the page
 * and the scope agree, except that BT and ET run through the points on a
 * monotone spline (like the Artisan export) rather than straight segments,
 * so the RoR has no steps at the points. Times are seconds from CHARGE,
 * temperatures °C.
 */
import type { ProfileWithPoints } from '#lib/api/profiles.js';
import type { SessionTelemetryPoint } from '#lib/api/sessions.js';
import {
	profileCurve,
	recordedCurve,
	profileRor,
	profileTicks,
	type Num,
	type ProfileTick
} from '#lib/scope/scopeData.js';
import { phaseRows, type PhaseRow, type Mark } from '#lib/roast/report.js';
import { fitMonotone, monotoneAt } from '#lib/utils/monotone.js';

/** Artisan's landmark names, in roast order. */
export type LandmarkCode =
	'CHARGE' | 'TP' | 'DRY' | 'FCs' | 'FCe' | 'SCs' | 'SCe' | 'DROP' | 'COOL';

export const LANDMARK_LABEL: Record<LandmarkCode, string> = {
	CHARGE: 'Charge',
	TP: 'Turning point',
	DRY: 'Dry end',
	FCs: 'First crack',
	FCe: 'First crack end',
	SCs: 'Second crack',
	SCe: 'Second crack end',
	DROP: 'Drop',
	COOL: 'Cool end'
};

const CODE_OF_EVENT: Record<string, LandmarkCode> = {
	charge: 'CHARGE',
	turning_point: 'TP',
	drying_end: 'DRY',
	first_crack_start: 'FCs',
	first_crack_end: 'FCe',
	second_crack_start: 'SCs',
	second_crack_end: 'SCe',
	drop: 'DROP',
	cool_end: 'COOL'
};

interface ProfileLandmark {
	code: LandmarkCode;
	t: number;
	bt: number | null;
	et: number | null;
	/** Where it comes from: a landmark event, the profile's targets, or the curve itself. */
	source: 'event' | 'target' | 'curve';
}

export interface ProfileView {
	t: number[];
	bt: Num[];
	et: Num[];
	btRor: Num[];
	etRor: Num[];
	fan: Num[];
	heater: Num[];
	/** The roast the profile was recorded from (faint background), when it has one. */
	recorded: { t: number[]; bt: Num[]; et: Num[] } | null;
	landmarks: ProfileLandmark[];
	/** Setpoint, fan, heater, mode and note events, placed in time. */
	controls: ProfileTick[];
	phases: PhaseRow[];
	/** CHARGE → DROP, s. */
	total: number | null;
	/** Development time ratio, %. */
	dtr: number | null;
	hasEt: boolean;
	hasFan: boolean;
	hasHeater: boolean;
}

/** Linear value of a sampled series at `t`. */
export function valueAt(ts: number[], vs: Num[], t: number): number | null {
	if (ts.length === 0 || t < ts[0] || t > ts[ts.length - 1]) return null;
	let lo = 0;
	let hi = ts.length - 1;
	while (hi - lo > 1) {
		const mid = (lo + hi) >> 1;
		if (ts[mid] <= t) lo = mid;
		else hi = mid;
	}
	const a = vs[lo];
	const b = vs[hi];
	if (a == null || b == null) return a ?? b ?? null;
	const span = ts[hi] - ts[lo];
	return span <= 0 ? a : a + ((b - a) * (t - ts[lo])) / span;
}

/** Index of the BT minimum if BT dips after CHARGE (a hot charge), else null. */
function dipIndex(bt: Num[]): number | null {
	if (bt.length < 3 || bt[0] == null) return null;
	let iMin = 0;
	for (let i = 1; i < Math.ceil(bt.length / 2); i++) {
		const v = bt[i];
		if (v != null && v < (bt[iMin] as number)) iMin = i;
	}
	return iMin > 0 && (bt[0] as number) - (bt[iMin] as number) > 0.5 ? iMin : null;
}

export function profileView(p: ProfileWithPoints): ProfileView {
	const pts = [...(p.points ?? [])].sort((a, b) => a.time_seconds - b.time_seconds);
	const rec = p.curves && p.curves.bt.length > 1 ? recordedCurve(p.curves) : null;
	// The points are the profile; recorded curves stand in only when there are none.
	const main = pts.length >= 2 ? profileCurve(pts) : (rec ?? { t: [], bt: [], et: [], fan: [] });
	if (pts.length >= 2) {
		const btFit = fitMonotone(pts.map((q) => [q.time_seconds, q.target_temp]));
		const etFit = fitMonotone(
			pts
				.filter((q) => q.target_env_temp != null)
				.map((q) => [q.time_seconds, q.target_env_temp as number])
		);
		main.bt = main.t.map((s, i) => (btFit ? monotoneAt(btFit, s) : main.bt[i]));
		main.et = main.t.map((s) => (etFit ? monotoneAt(etFit, s) : null));
	}
	const { t, bt, et, fan } = main;

	// Heater, held from point to point like the fan.
	const heater: Num[] = [];
	if (pts.length >= 2) {
		let k = 0;
		for (const s of t) {
			while (k + 1 < pts.length && pts[k + 1].time_seconds <= s) k++;
			heater.push(pts[k].heater_pct ?? null);
		}
	} else {
		const h = p.curves?.heater;
		for (let i = 0; i < t.length; i++) heater.push(h && h.length === t.length ? h[i] : null);
	}

	const hasEt = et.some((v) => v != null);
	const btRor = profileRor(t, bt);
	const etRor = hasEt ? profileRor(t, et) : t.map(() => null);
	const ticks = profileTicks(p.events ?? [], main);

	// Landmarks: events first, then the profile's targets and the curve.
	const at = (
		time: number,
		source: ProfileLandmark['source'],
		code: LandmarkCode
	): ProfileLandmark => ({
		code,
		t: time,
		bt: valueAt(t, bt, time),
		et: hasEt ? valueAt(t, et, time) : null,
		source
	});
	const found = new Map<LandmarkCode, ProfileLandmark>();
	for (const tick of ticks) {
		if (tick.action !== 'landmark') continue;
		const code = CODE_OF_EVENT[tick.event.label ?? ''];
		if (code && !found.has(code)) found.set(code, at(tick.t, 'event', code));
	}
	const end = t.length ? t[t.length - 1] : null;
	if (t.length && t[0] <= 0 && !found.has('CHARGE')) found.set('CHARGE', at(0, 'curve', 'CHARGE'));
	if (!found.has('TP')) {
		const i = dipIndex(bt);
		if (i != null) found.set('TP', at(t[i], 'curve', 'TP'));
	}
	if (
		!found.has('FCs') &&
		p.target_first_crack != null &&
		end != null &&
		p.target_first_crack <= end
	) {
		found.set('FCs', at(p.target_first_crack, 'target', 'FCs'));
	}
	if (!found.has('DROP') && end != null) {
		const total =
			p.target_total_time != null && p.target_total_time <= end ? p.target_total_time : end;
		found.set('DROP', at(total, p.target_total_time != null ? 'target' : 'curve', 'DROP'));
	}
	const landmarks = [...found.values()].sort((a, b) => a.t - b.t);

	// Phases and development, as the roast report computes them.
	const mark = (code: LandmarkCode): Mark | null => {
		const l = found.get(code);
		return l ? { t: l.t, temp: l.bt } : null;
	};
	const drop = mark('DROP');
	const fc = mark('FCs');
	const total = drop?.t ?? null;
	const dtr = fc && total && total > fc.t ? ((total - fc.t) / total) * 100 : null;
	const asTelemetry = t.map(
		(s, i) => ({ elapsed_seconds: s, bean_temp: bt[i] }) as SessionTelemetryPoint
	);
	const phases = phaseRows(
		{ chargeAt: 0, tp: mark('TP'), dry: mark('DRY'), fc, drop, total, dtr },
		asTelemetry
	);

	return {
		t,
		bt,
		et,
		btRor,
		etRor,
		fan,
		heater,
		recorded: rec && pts.length >= 2 ? { t: rec.t, bt: rec.bt, et: rec.et } : null,
		landmarks,
		controls: ticks.filter((x) => x.action !== 'landmark'),
		phases,
		total,
		dtr,
		hasEt,
		hasFan: fan.some((v) => v != null),
		hasHeater: heater.some((v) => v != null)
	};
}

/** The number a control event sets, for a table or a chart label ("SP 185", "Fan 85 %"). */
export function controlLabel(tick: ProfileTick, fmtTemp: (c: number) => string): string {
	const e = tick.event;
	switch (e.action) {
		case 'setpoint':
			return e.value != null ? `Setpoint ${fmtTemp(e.value)}` : 'Setpoint';
		case 'fan':
			return e.value != null ? `Fan ${Math.round(e.value)} %` : 'Fan';
		case 'heater':
			return e.value != null ? `Heater ${Math.round(e.value)} %` : 'Heater';
		case 'mode':
			return `Mode ${e.label ?? ''}`.trim();
		case 'note':
			return e.label ?? 'Note';
		default:
			return e.action;
	}
}
