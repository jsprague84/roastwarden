/**
 * Pure data shaping for the live scope. No DOM, no stores: everything here is
 * unit-testable and runs in a few milliseconds for two hours of 1 Hz data.
 *
 * Time basis: the scope's x axis is seconds from an *origin*. The origin is
 * CHARGE when the session has one (Artisan's convention: pre-charge time is
 * negative), else the session start, else the first sample.
 * Temperatures stay °C here; the component converts for display.
 */
import type { Telemetry } from '#lib/api/telemetry.js';

export interface HistoryEntry {
	timestamp: number;
	telemetry: Telemetry;
}

export interface Landmark {
	type: string;
	/** Seconds since the session start (how the server stores events). */
	elapsed_seconds: number;
	temperature?: number;
}

export type Num = number | null;

export interface ScopeSeries {
	/** Seconds from origin. */
	t: number[];
	bt: Num[];
	et: Num[];
	rorBt: Num[];
	rorEt: Num[];
	sp: Num[];
	/** Heater %, 0..100. */
	heater: Num[];
	/** Fan %, 0..100. */
	fan: Num[];
}

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/** Choose the x-axis origin: CHARGE > session start > first sample. */
export function resolveOrigin(
	history: HistoryEntry[],
	sessionStartMs: number | null,
	landmarks: Landmark[],
	previewAtNow = false
): { originMs: number; chargeAtOrigin: boolean } {
	const charge = landmarks.find((l) => l.type === 'charge');
	if (sessionStartMs != null && charge) {
		return { originMs: sessionStartMs + charge.elapsed_seconds * 1000, chargeAtOrigin: true };
	}
	if (sessionStartMs != null) return { originMs: sessionStartMs, chargeAtOrigin: false };
	// No roast yet but a profile picked: draw it as if you charged now, so it
	// sits ahead of the live trace instead of at the start of the buffer
	// (which reaches back up to two hours on a roaster that streams all day).
	if (previewAtNow && history.length > 0)
		return { originMs: history[history.length - 1].timestamp, chargeAtOrigin: false };
	return { originMs: history[0]?.timestamp ?? Date.now(), chargeAtOrigin: false };
}

/**
 * Offsets (s) after "now" where the scope evaluates the future: every
 * `step` s up to the projection `horizon`, then the profile every
 * `profileStep` s to its end. Without the profile grid the overlay stopped
 * at now + horizon, so most of the profile was never drawn.
 */
export function futureOffsets(
	nowSec: number,
	horizon: number,
	step: number,
	profileT: number[] | null,
	profileStep = 5
): number[] {
	const out = new Set<number>();
	for (let k = step; k <= horizon; k += step) out.add(k);
	if (profileT && profileT.length > 0) {
		const t0 = profileT[0];
		const end = profileT[profileT.length - 1];
		if (end > nowSec) {
			// Grid points on the profile's own time base, strictly after now.
			const first = t0 + Math.max(0, Math.floor((nowSec - t0) / profileStep) + 1) * profileStep;
			for (let t = first; t < end; t += profileStep) {
				const k = Math.round((t - nowSec) * 1000) / 1000;
				if (k > 0) out.add(k);
			}
			out.add(Math.round((end - nowSec) * 1000) / 1000);
		}
	}
	return [...out].filter((k) => k > 0).sort((a, b) => a - b);
}

/** Column-major series for uPlot, x in seconds from `originMs`. */
export function buildScopeSeries(history: HistoryEntry[], originMs: number): ScopeSeries {
	const n = history.length;
	const out: ScopeSeries = {
		t: new Array(n),
		bt: new Array(n),
		et: new Array(n),
		rorBt: new Array(n),
		rorEt: new Array(n),
		sp: new Array(n),
		heater: new Array(n),
		fan: new Array(n)
	};
	for (let i = 0; i < n; i++) {
		const h = history[i];
		const tm = h.telemetry;
		out.t[i] = (h.timestamp - originMs) / 1000;
		out.bt[i] = finite(tm.beanTemp) ? tm.beanTemp : null;
		out.et[i] = finite(tm.envTemp) ? tm.envTemp : null;
		out.rorBt[i] = finite(tm.rateOfRise) ? tm.rateOfRise : null;
		out.rorEt[i] = finite(tm.rateOfRiseEt) ? tm.rateOfRiseEt : null;
		out.sp[i] = finite(tm.setpoint) && tm.setpoint > 0 ? tm.setpoint : null;
		out.heater[i] = finite(tm.heater) ? tm.heater : null;
		out.fan[i] = finite(tm.fan) ? tm.fan : null;
	}
	return out;
}

/** A declared channel as the scope needs it (docs/channels.md). */
export interface ScopeChannel {
	key: string;
	label: string;
	/** `C`, `%` or another unit. */
	unit: string;
	max: number | null;
}

export interface ChannelLine {
	key: string;
	label: string;
	/** uPlot scale: `temp` (°C, converted for display), `pct`, or `ch:<key>` of its own. */
	scale: string;
	axis: 'temp' | 'pct' | 'own';
	/** Per sample, display units (°C converted by `toTemp`), null where the sample has none. */
	values: Num[];
	/** Top of an own scale: the channel's `max`, else the highest value seen (at least 1). */
	top: number | null;
}

/**
 * Each declared channel's line over `values` (one sample object per point: live telemetry or
 * a stored sample's extras): a `C` channel on the temperature axis converted by `toTemp`
 * (°C → the owner's unit), `%` on the output axis, another unit on a scale of its own.
 */
export function channelLines(
	samples: unknown[],
	channels: ScopeChannel[],
	toTemp: (c: number) => number = (c) => c
): ChannelLine[] {
	return channels.map((c) => {
		const axis = c.unit === 'C' ? 'temp' : c.unit === '%' ? 'pct' : 'own';
		const values: Num[] = samples.map((s) => {
			const v = s && typeof s === 'object' ? (s as Record<string, unknown>)[c.key] : undefined;
			if (typeof v !== 'number' || !Number.isFinite(v)) return null;
			return axis === 'temp' ? toTemp(v) : v;
		});
		let top: number | null = null;
		if (axis === 'own') {
			const seen = values.reduce<number>((m, v) => (v != null && v > m ? v : m), 0);
			top = c.max != null && c.max > 0 ? c.max : Math.max(1, seen);
		}
		return {
			key: c.key,
			label: c.label,
			scale: axis === 'own' ? `ch:${c.key}` : axis,
			axis,
			values,
			top
		};
	});
}

/** Convert landmark elapsed (from session start) to scope seconds. */
export function landmarkSeconds(
	l: Landmark,
	sessionStartMs: number | null,
	originMs: number
): number {
	if (sessionStartMs == null) return l.elapsed_seconds;
	return (sessionStartMs + l.elapsed_seconds * 1000 - originMs) / 1000;
}

export type PhaseKind = 'drying' | 'maillard' | 'development' | 'cooling';

export interface PhaseBand {
	kind: PhaseKind;
	label: string;
	from: number;
	to: number;
	/** True while the phase has not ended yet (extends to "now"). */
	open: boolean;
}

/**
 * Roast phases from landmarks, Artisan style: drying CHARGE→DRY, Maillard
 * DRY→FC, development FC→DROP, cooling DROP→COOL. The current phase is open
 * and extends to `nowSec`.
 */
export function phaseBands(
	landmarks: Landmark[],
	sessionStartMs: number | null,
	originMs: number,
	nowSec: number
): PhaseBand[] {
	const at = (type: string) => {
		const l = landmarks.find((x) => x.type === type);
		return l ? landmarkSeconds(l, sessionStartMs, originMs) : null;
	};
	const charge = at('charge');
	if (charge == null) return [];
	const dry = at('drying_end');
	const fc = at('first_crack_start');
	const drop = at('drop');
	const cool = at('cool_end');
	const bands: PhaseBand[] = [];
	const push = (kind: PhaseKind, label: string, from: number | null, to: number | null) => {
		if (from == null) return;
		const end = to ?? nowSec;
		if (end <= from) return;
		bands.push({ kind, label, from, to: end, open: to == null });
	};
	push('drying', 'Drying', charge, dry ?? fc ?? drop);
	if (dry != null) push('maillard', 'Maillard', dry, fc ?? drop);
	if (fc != null) push('development', 'Development', fc, drop);
	if (drop != null) push('cooling', 'Cooling', drop, cool);
	return bands;
}

/** Percent of a band within charge→(drop|now); used for the phase labels. */
export function phasePercents(bands: PhaseBand[]): Record<PhaseKind, number> {
	const roast = bands.filter((b) => b.kind !== 'cooling');
	const total = roast.reduce((s, b) => s + (b.to - b.from), 0);
	const out: Record<PhaseKind, number> = { drying: 0, maillard: 0, development: 0, cooling: 0 };
	if (total <= 0) return out;
	for (const b of roast) out[b.kind] = Math.round(((b.to - b.from) / total) * 100);
	return out;
}

/** Ordinary least squares over the last `windowSec` seconds. */
export function fitLine(
	t: number[],
	v: Num[],
	windowSec: number,
	fromSec = -Infinity
): { slope: number; intercept: number; n: number } | null {
	if (t.length === 0) return null;
	const tEnd = t[t.length - 1];
	const tStart = Math.max(tEnd - windowSec, fromSec);
	let sx = 0,
		sy = 0,
		sxx = 0,
		sxy = 0,
		n = 0;
	for (let i = t.length - 1; i >= 0; i--) {
		if (t[i] < tStart) break;
		const y = v[i];
		if (y == null || !Number.isFinite(y)) continue;
		const x = t[i] - tEnd;
		sx += x;
		sy += y;
		sxx += x * x;
		sxy += x * y;
		n++;
	}
	if (n < 3) return null;
	const den = n * sxx - sx * sx;
	if (Math.abs(den) < 1e-9) return null;
	const slope = (n * sxy - sx * sy) / den; // per second, relative to tEnd
	const intercept = (sy - slope * sx) / n; // value at tEnd
	return { slope, intercept, n };
}

export interface Projection {
	/** [tStart, vStart] and [tEnd, vEnd] in scope seconds / °C. */
	from: [number, number];
	to: [number, number];
	/** Value at +60 s and +120 s, for the readout. */
	at60: number;
	at120: number;
}

/**
 * Seconds after CHARGE before the scope projects. The first seconds hold the charge drop
 * (or, on a cold charge, the steepest climb of the roast), so a fit over them sent the
 * projections swinging; with this the fit also never reaches back across CHARGE.
 */
export const PROJECT_AFTER_CHARGE_SEC = 30;

/**
 * Project BT forward using a quadratic model: current value, current RoR
 * (°C/min → °C/s) and the RoR trend (slope of RoR over the fit window).
 * Linear when the RoR is steady or rising (see `accel` below).
 */
export function projectBt(
	t: number[],
	bt: Num[],
	ror: Num[],
	horizonSec: number,
	windowSec = 30,
	/** No projection before this time, and no fit over samples before it. */
	fromSec = -Infinity
): Projection | null {
	if (t.length < 3) return null;
	const i = t.length - 1;
	if (t[i] < fromSec) return null;
	const b0 = bt[i];
	if (b0 == null) return null;
	const rorFit = fitLine(t, ror, windowSec, fromSec);
	const r0 = ror[i] ?? rorFit?.intercept ?? null;
	if (r0 == null) return null;
	const rPerSec = r0 / 60;
	// Only a falling RoR bends the projection. A rising one (the climb after CHARGE, a fan
	// cut) is short-lived, and extrapolating it sent the projection up steeply.
	const accel = Math.min(0, rorFit?.slope ?? 0) / 60; // °C/s per s
	// Integrate r(t) = r0 + a·t, but never below zero: once the projected RoR
	// reaches 0 the bean temperature holds instead of falling, which is what a
	// roast does when the heat is cut (Artisan's projection has the same floor).
	const tZero = accel < 0 && rPerSec > 0 ? -rPerSec / accel : Infinity;
	const at = (dt: number) => {
		const d = Math.min(dt, tZero);
		return b0 + rPerSec * d + 0.5 * accel * d * d;
	};
	return {
		from: [t[i], b0],
		to: [t[i] + horizonSec, at(horizonSec)],
		at60: at(60),
		at120: at(120)
	};
}

/** Project RoR forward linearly from its recent trend. */
export function projectRor(
	t: number[],
	ror: Num[],
	horizonSec: number,
	windowSec = 30,
	fromSec = -Infinity
): Projection | null {
	const i = t.length - 1;
	if (i < 0 || t[i] < fromSec) return null;
	const fit = fitLine(t, ror, windowSec, fromSec);
	if (!fit) return null;
	const r0 = ror[i] ?? fit.intercept;
	const at = (dt: number) => r0 + fit.slope * dt;
	return {
		from: [t[i], r0],
		to: [t[i] + horizonSec, at(horizonSec)],
		at60: at(60),
		at120: at(120)
	};
}

/** Seconds until BT reaches `target` at the current projection, or null if it won't within `maxSec`. */
export function etaToTemp(p: Projection | null, target: number, maxSec = 900): number | null {
	if (!p) return null;
	const [t0, v0] = p.from;
	const [t1, v1] = p.to;
	const slope = (v1 - v0) / (t1 - t0);
	if (!(slope > 0) || v0 >= target) return null;
	const dt = (target - v0) / slope;
	return dt <= maxSec ? dt : null;
}

export interface ProfilePointLike {
	time_seconds: number;
	target_temp: number;
	target_env_temp?: number | null;
	fan_speed?: number | null;
}

export interface ProfileCurveData {
	t: number[];
	bt: Num[];
	et: Num[];
	fan: Num[];
}

/*
 * Profile placement. Profile time 0 is CHARGE, and so is scope time 0 (the
 * origin is CHARGE, or the session start standing in for it), so a profile
 * lands at scope time = profile time + `shiftSec`, where `shiftSec` is the
 * replay's align offset (live landmark time − profile landmark time; 0 when
 * aligned on CHARGE or when no replay runs).
 */

/** Profile curve resampled to 1 s, in scope seconds. */
export function profileCurve(points: ProfilePointLike[], shiftSec = 0): ProfileCurveData {
	const pts = points.slice().sort((a, b) => a.time_seconds - b.time_seconds);
	if (pts.length === 0) return { t: [], bt: [], et: [], fan: [] };
	const shift = shiftSec;
	const t: number[] = [];
	const bt: Num[] = [];
	const et: Num[] = [];
	const fan: Num[] = [];
	const lerp = (a: number, b: number, f: number) => a + (b - a) * f;
	for (let k = 0; k < pts.length - 1; k++) {
		const a = pts[k];
		const b = pts[k + 1];
		const span = Math.max(1, b.time_seconds - a.time_seconds);
		for (let s = a.time_seconds; s < b.time_seconds; s++) {
			const f = (s - a.time_seconds) / span;
			t.push(s + shift);
			bt.push(lerp(a.target_temp, b.target_temp, f));
			et.push(
				a.target_env_temp != null && b.target_env_temp != null
					? lerp(a.target_env_temp, b.target_env_temp, f)
					: null
			);
			fan.push(a.fan_speed != null ? a.fan_speed : null);
		}
	}
	const last = pts[pts.length - 1];
	t.push(last.time_seconds + shift);
	bt.push(last.target_temp);
	et.push(last.target_env_temp ?? null);
	fan.push(last.fan_speed ?? null);
	return { t, bt, et, fan };
}

export interface RecordedCurvesLike {
	interval_seconds: number;
	bt: Num[];
	et?: Num[];
	fan?: Num[];
}

/**
 * A profile's recorded background curves (one value per interval from CHARGE)
 * in scope seconds, the same shape as `profileCurve`.
 */
export function recordedCurve(c: RecordedCurvesLike, shiftSec = 0): ProfileCurveData {
	const shift = shiftSec;
	const step = c.interval_seconds > 0 ? c.interval_seconds : 1;
	const n = c.bt.length;
	const pick = (arr: Num[] | undefined) =>
		arr && arr.length === n ? arr.slice() : new Array<Num>(n).fill(null);
	return {
		t: Array.from({ length: n }, (_, i) => i * step + shift),
		bt: c.bt.slice(),
		et: pick(c.et),
		fan: pick(c.fan)
	};
}

export interface ProfileEventLike {
	trigger_kind: 'time' | 'bt';
	at_seconds: number | null;
	at_bt: number | null;
	action: string;
	value: number | null;
	label: string | null;
}

export interface ProfileTick {
	/** Scope seconds. */
	t: number;
	action: string;
	event: ProfileEventLike;
}

export type TriggerPolicy = 'profile' | 'time' | 'bt';

/** The trigger an event uses under a replay's policy (falls back to what the event has). */
export function effectiveTrigger(
	e: ProfileEventLike,
	policy: TriggerPolicy = 'profile'
): 'time' | 'bt' {
	const want = policy === 'profile' ? e.trigger_kind : policy;
	if (want === 'time' && e.at_seconds == null) return 'bt';
	if (want === 'bt' && e.at_bt == null) return 'time';
	return want;
}

/**
 * Where each profile event lands on the scope: time triggers at their time,
 * BT triggers where the profile curve (already shifted) rises through their
 * BT after its minimum (the turning point). Events that never fire are dropped.
 */
export function profileTicks(
	events: ProfileEventLike[],
	curve: ProfileCurveData | null,
	shiftSec = 0,
	policy: TriggerPolicy = 'profile'
): ProfileTick[] {
	const shift = shiftSec;
	let iMin = -1;
	if (curve) {
		for (let i = 0; i < curve.bt.length; i++) {
			const v = curve.bt[i];
			if (v != null && (iMin < 0 || v < (curve.bt[iMin] as number))) iMin = i;
		}
	}
	const out: ProfileTick[] = [];
	for (const e of events) {
		if (effectiveTrigger(e, policy) === 'time') {
			if (e.at_seconds != null) out.push({ t: e.at_seconds + shift, action: e.action, event: e });
			continue;
		}
		if (!curve || iMin < 0 || e.at_bt == null) continue;
		for (let i = Math.max(1, iMin + 1); i < curve.bt.length; i++) {
			const a = curve.bt[i - 1];
			const b = curve.bt[i];
			if (a == null || b == null) continue;
			if (a <= e.at_bt && b >= e.at_bt) {
				const f = b === a ? 1 : (e.at_bt - a) / (b - a);
				out.push({
					t: curve.t[i - 1] + f * (curve.t[i] - curve.t[i - 1]),
					action: e.action,
					event: e
				});
				break;
			}
		}
	}
	return out.sort((x, y) => x.t - y.t);
}

/**
 * Align a secondary curve onto the main time base (nearest-previous sample,
 * null outside its range), so uPlot can fill a band between them.
 */
/**
 * A profile's rate of rise (°C/min) from its BT curve: the slope across
 * ±`halfWindow` seconds, so a curve built from straight segments shows a
 * smooth RoR instead of steps.
 */
export function profileRor(t: number[], bt: Num[], halfWindow = 15): Num[] {
	const out: Num[] = new Array(t.length).fill(null);
	let lo = 0;
	let hi = 0;
	for (let i = 0; i < t.length; i++) {
		while (lo < i && t[lo] < t[i] - halfWindow) lo++;
		if (hi < i) hi = i;
		while (hi + 1 < t.length && t[hi + 1] <= t[i] + halfWindow) hi++;
		const a = bt[lo];
		const b = bt[hi];
		const span = t[hi] - t[lo];
		if (a == null || b == null || span < 1) continue;
		out[i] = ((b - a) / span) * 60;
	}
	return out;
}

export function alignToBase(baseT: number[], srcT: number[], srcV: Num[]): Num[] {
	const out: Num[] = new Array(baseT.length).fill(null);
	if (srcT.length === 0) return out;
	let j = 0;
	for (let i = 0; i < baseT.length; i++) {
		const x = baseT[i];
		if (x < srcT[0] || x > srcT[srcT.length - 1]) continue;
		while (j + 1 < srcT.length && srcT[j + 1] <= x) j++;
		const a = srcV[j];
		const b = srcV[Math.min(j + 1, srcT.length - 1)];
		if (a == null) continue;
		if (b == null || srcT[j + 1] === undefined || srcT[j + 1] === srcT[j]) {
			out[i] = a;
		} else {
			const f = (x - srcT[j]) / (srcT[j + 1] - srcT[j]);
			out[i] = a + (b - a) * f;
		}
	}
	return out;
}

export interface EventFlag {
	t: number;
	kind: 'heater' | 'fan' | 'sp';
	value: number;
}

/** Changes in heater / fan / setpoint, like Artisan's event flags. */
export function eventFlags(s: ScopeSeries, maxFlags = 200): EventFlag[] {
	const flags: EventFlag[] = [];
	let lastH: Num = null,
		lastF: Num = null,
		lastS: Num = null;
	for (let i = 0; i < s.t.length; i++) {
		const h = s.heater[i],
			f = s.fan[i],
			sp = s.sp[i];
		if (h != null && (lastH == null || Math.abs(h - lastH) >= 1)) {
			if (lastH != null) flags.push({ t: s.t[i], kind: 'heater', value: h });
			lastH = h;
		}
		if (f != null && (lastF == null || Math.abs(f - lastF) >= 1)) {
			if (lastF != null) flags.push({ t: s.t[i], kind: 'fan', value: f });
			lastF = f;
		}
		// A follower nudges the setpoint by fractions of a degree every second;
		// only jumps of 5 °C or more count as an operator event.
		if (sp != null && (lastS == null || Math.abs(sp - lastS) >= 5)) {
			if (lastS != null) flags.push({ t: s.t[i], kind: 'sp', value: sp });
			lastS = sp;
		}
	}
	return flags.length > maxFlags ? flags.slice(flags.length - maxFlags) : flags;
}

/** mm:ss with a sign for pre-charge time. */
export function fmtClock(sec: number): string {
	const neg = sec < 0;
	const a = Math.abs(Math.round(sec));
	const m = Math.floor(a / 60);
	const s = a % 60;
	return `${neg ? '-' : ''}${m}:${s.toString().padStart(2, '0')}`;
}

/** What the scope's time axis shows when the user has not zoomed. */
export type ScopeView = 'roast' | 'live10' | 'live30' | 'all';

/** A roast frame is at least this long, so the axis is stable from CHARGE on. */
const ROAST_FRAME_SEC = 15 * 60;

export interface ViewInputs {
	/** First and latest sample, seconds on the scope's time base. */
	dataMin: number;
	nowSec: number;
	/** Session start on the time base (negative when CHARGE is the origin). */
	sessionStartSec: number | null;
	/** Last point of the profile overlay, if one is loaded. */
	profileEndSec: number | null;
	/** Seconds of projection drawn ahead of now. */
	horizon: number;
}

/**
 * The x range for a view. `roast` frames the session: a little before it
 * started, and on to the latest of the profile's end, now plus the
 * projection, or a 15 min frame, rounded up to a whole minute so the axis
 * does not creep every second. `live10`/`live30` roll with now; `all` shows
 * everything the browser holds.
 */
export function viewRange(view: ScopeView, o: ViewInputs): [number, number] {
	const ahead = o.nowSec + o.horizon;
	if (view === 'roast' && o.sessionStartSec != null) {
		const min = o.sessionStartSec - 15;
		const want = Math.max(ahead, (o.profileEndSec ?? -Infinity) + 60, min + ROAST_FRAME_SEC);
		return [min, Math.ceil(want / 60) * 60];
	}
	if (view === 'all') return [Math.min(o.dataMin, o.nowSec), ahead];
	const win = view === 'live30' ? 30 * 60 : 10 * 60;
	return [o.nowSec - win, ahead];
}

export interface DevGuide {
	/** Seconds since first crack. */
	devSec: number;
	/** Development time ratio now, %. */
	dtr: number;
	/** BT rise since first crack, °C (null without both readings). */
	dT: number | null;
	targetDtr: number;
	/** Scope time at which DROP gives `targetDtr`. */
	dropAtSec: number;
	/** Seconds until then (negative: past it). */
	secsToDrop: number;
	/** BT at that DROP if the RoR held (an upper estimate: RoR usually keeps falling). */
	btAtDrop: number | null;
	/** Seconds until BT reaches `targetDropBt` at the current RoR. */
	secsToDropBt: number | null;
	/** The profile's drop BT, when it has one. */
	targetDropBt: number | null;
}

/**
 * Development guidance once first crack is marked: dev time, live DTR, ΔT,
 * and when to drop for a target DTR. Times are scope seconds; `chargeSec`
 * is where roast time zero sits on that axis.
 */
export function developmentGuide(o: {
	chargeSec: number;
	fcSec: number | null;
	nowSec: number;
	btNow: number | null;
	btFc: number | null;
	/** °C/min. */
	ror: number | null;
	targetDtr: number;
	targetDropBt?: number | null;
}): DevGuide | null {
	if (o.fcSec == null || o.nowSec < o.fcSec) return null;
	const toFc = o.fcSec - o.chargeSec;
	const total = o.nowSec - o.chargeSec;
	if (toFc <= 0 || total <= 0) return null;
	const d = Math.min(Math.max(o.targetDtr, 1), 60) / 100;
	const devSec = o.nowSec - o.fcSec;
	const dropAtSec = o.chargeSec + toFc / (1 - d);
	const secsToDrop = dropAtSec - o.nowSec;
	const rate = o.ror != null && Number.isFinite(o.ror) ? o.ror / 60 : null;
	const btAtDrop =
		o.btNow != null && rate != null ? o.btNow + rate * Math.max(0, secsToDrop) : null;
	let secsToDropBt: number | null = null;
	if (
		o.targetDropBt != null &&
		o.btNow != null &&
		rate != null &&
		rate > 0 &&
		o.btNow < o.targetDropBt
	) {
		secsToDropBt = (o.targetDropBt - o.btNow) / rate;
	}
	return {
		devSec,
		dtr: (devSec / total) * 100,
		dT: o.btNow != null && o.btFc != null ? o.btNow - o.btFc : null,
		targetDtr: d * 100,
		dropAtSec,
		secsToDrop,
		btAtDrop,
		secsToDropBt,
		targetDropBt: o.targetDropBt ?? null
	};
}

/** A profile's development ratio from its first crack and total time, %. */
export function profileDtr(
	firstCrackSec: number | null | undefined,
	totalSec: number | null | undefined
): number | null {
	if (!firstCrackSec || !totalSec || totalSec <= firstCrackSec) return null;
	return ((totalSec - firstCrackSec) / totalSec) * 100;
}

/** Where a label went: its row, which of its texts fitted (0 = the first, longest), its left edge. */
export interface PlacedLabel {
	row: number;
	choice: number;
	left: number;
}

/**
 * Lay out labels centred on x positions along an edge, in `rows` rows: each label takes the
 * first row where it clears the labels already there by `gap`, trying its texts longest
 * first (`widths`), and is dropped (null) when none fits. Labels are kept inside [lo, hi].
 * Items must be in x order. Landmarks marked close together (Charge, TP, Dry end on a phone)
 * used to overlap because rows only alternated.
 */
export function stackLabels(
	items: { x: number; widths: number[] }[],
	rows: number,
	lo: number,
	hi: number,
	gap = 4
): (PlacedLabel | null)[] {
	const rowEnd = Array.from({ length: rows }, () => -Infinity);
	return items.map(({ x, widths }) => {
		for (let choice = 0; choice < widths.length; choice++) {
			const w = widths[choice];
			const left = Math.min(Math.max(x - w / 2, lo), hi - w);
			for (let row = 0; row < rows; row++) {
				if (left >= rowEnd[row] + gap) {
					rowEnd[row] = left + w;
					return { row, choice, left };
				}
			}
		}
		return null;
	});
}
