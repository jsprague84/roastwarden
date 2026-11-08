/**
 * The live scope's model: from the telemetry history, the landmarks, the profile and the
 * drive's alignment to what uPlot draws each second. The aligned data series in display units
 * (°C → display happens here, once per tick), the projections, the model's prediction on the
 * same grid, the phase bands and marks, the event flags, the profile's event ticks, the view's
 * x range, and the readout's numbers.
 */
import type { AlignedData } from 'uplot';
import type { Prediction } from '#lib/api/predict.js';
import type { ProfileWithPoints } from '#lib/api/profiles.js';
import type { ReplayState } from '#lib/api/replay.js';
import { expectationsFor } from '#lib/guidance/expect.js';
import type { HeaterLine } from '#lib/stores/chartPrefs.svelte.js';
import { deltaToDisplay, toDisplay } from '#lib/stores/units.svelte.js';
import type { ScopeMark } from './draw.js';
import { predictionOnGrid } from './prediction.js';
import {
	PROJECT_AFTER_CHARGE_SEC,
	alignToBase,
	buildScopeSeries,
	channelLines,
	developmentGuide,
	etaToTemp,
	eventFlags,
	fmtClock,
	futureOffsets,
	landmarkSeconds,
	phaseBands,
	phasePercents,
	profileCurve,
	profileDtr,
	profileRor,
	profileTicks,
	projectBt,
	projectRor,
	recordedCurve,
	resolveOrigin,
	viewRange,
	type ChannelLine,
	type DevGuide,
	type EventFlag,
	type HistoryEntry,
	type Landmark,
	type Num,
	type PhaseBand,
	type PhaseKind,
	type ProfileTick,
	type ScopeChannel,
	type ScopeView
} from './scopeData.js';
import { HEATER_SMOOTH_SEC, smoothByTime } from './smooth.js';

// ---- series layout (index into uPlot data) ----
export const SERIES = {
	bt: 1,
	et: 2,
	sp: 3,
	rorBt: 4,
	rorEt: 5,
	heater: 6,
	fan: 7,
	profBt: 8,
	profEt: 9,
	projBt: 10,
	projRor: 11,
	profRor: 13,
	model: 14
} as const;
export type SeriesKey = keyof typeof SERIES;
/** Hidden series: profile BT only where live BT exists, so the delta band never reaches past "now". */
export const BAND = 12;
/** Hidden series: the model's 90 % band (docs/prediction.md). */
export const MODEL_LO = 15;
export const MODEL_HI = 16;
/** The first declared channel's index in the uPlot data (docs/channels.md). */
export const CH0 = 17;
/** The series the toggles below the scope switch, in that order. */
export const TOGGLEABLE: SeriesKey[] = [
	'bt',
	'et',
	'sp',
	'rorBt',
	'rorEt',
	'heater',
	'fan',
	'profBt',
	'profEt',
	'profRor',
	'projBt',
	'projRor',
	'model'
];

/** Projection seconds ahead of now, and the step of the x grid there. */
const HORIZON = 120;
const HORIZON_STEP = 10;

export interface ScopeInput {
	history: HistoryEntry[];
	/** The roast's start, ms since the epoch; null without a roast. */
	sessionStartMs: number | null;
	landmarks: Landmark[];
	/** No roast yet, a profile picked: the scope previews it from "now". */
	previewing: boolean;
	profile: ProfileWithPoints | null;
	/** The drive running for this roast: it decides the profile's alignment and trigger. */
	replay: Pick<ReplayState, 'align' | 'config'> | null;
	/** The model's live prediction (docs/prediction.md), drawn between CHARGE and DROP; null
	 * while its series is off. */
	prediction: Prediction | null;
	/** The heater line as drawn: the PID's output, or a 15 s average (Settings → This browser). */
	heaterLine: HeaterLine;
	channels: ScopeChannel[];
	/** Development target when the profile has none (the `target_dtr_pct` setting). */
	targetDtr: number;
	view: ScopeView;
}

/** The expected development window (roast guidance), scope seconds and seconds after FC. */
export interface DevWindow {
	from: number;
	to: number;
	lo: number;
	hi: number;
}

/** The readout's numbers, °C and °C/min (the readout converts). */
export interface ScopeStats {
	bt: number;
	et: number;
	ror: Num;
	/** The profile's RoR now. */
	rorTarget: Num;
	/** BT − profile now; null while previewing or without a profile. */
	delta: Num;
	proj60: Num;
	proj120: Num;
	prediction: Prediction | null;
	/** Seconds until BT reaches `fcTarget` on the projection. */
	etaFc: number | null;
	fcTarget: number;
	dev: DevGuide | null;
	devWindow: DevWindow | null;
	/** RMS of BT − profile since the turning point (else CHARGE + 60 s), 10 samples or more. */
	trackRms: Num;
}

export interface ScopeModel {
	data: AlignedData;
	/** Live samples; the rest of each series is the future (projections, the profile). */
	n: number;
	nowSec: number;
	xRange: [number, number];
	chLines: ChannelLine[];
	chargeAtOrigin: boolean;
	bands: PhaseBand[];
	pct: Record<PhaseKind, number>;
	marks: ScopeMark[];
	flags: EventFlag[];
	ticks: ProfileTick[];
	stats: ScopeStats;
}

/** The scope's model for one tick, or null before the first sample. */
export function scopeModel(o: ScopeInput): ScopeModel | null {
	const h = o.history;
	if (h.length === 0) return null;
	const { landmarks, sessionStartMs, previewing } = o;
	const { originMs, chargeAtOrigin } = resolveOrigin(h, sessionStartMs, landmarks, previewing);
	const s = buildScopeSeries(h, originMs);
	const nowSec = s.t[s.t.length - 1];

	const T = (v: Num) => (v == null ? null : toDisplay(v));
	const D = (v: Num) => (v == null ? null : deltaToDisplay(v));

	// Projections (computed in °C, then converted)
	// After CHARGE (t = 0), only once the charge has settled; before it, as the trace runs.
	// None once DROP is marked: the beans are out, so BT and RoR fall away from any forecast.
	const projectFrom = chargeAtOrigin ? PROJECT_AFTER_CHARGE_SEC : -Infinity;
	const dropped = landmarks.some((l) => l.type === 'drop');
	const pBt = dropped ? null : projectBt(s.t, s.bt, s.rorBt, HORIZON, 30, projectFrom);
	const pRor = dropped ? null : projectRor(s.t, s.rorBt, HORIZON, 30, projectFrom);

	// Profile overlay aligned to the live time base (plus horizon)
	const prof = o.profile;
	const alignShift = o.replay?.align?.offset_seconds ?? 0;
	// A profile saved from a roast carries its full recorded curve; prefer it to the points.
	const curve = prof
		? prof.curves?.bt?.length
			? recordedCurve(prof.curves, alignShift)
			: profileCurve(prof.points, alignShift)
		: null;
	const ticks = prof?.events?.length
		? profileTicks(prof.events, curve, alignShift, o.replay?.config.trigger ?? 'profile')
		: [];

	// x: live times, then the future: projection steps to the horizon and the
	// profile every 5 s to its end (the whole profile stays drawn)
	const x = s.t.slice();
	const n = x.length;
	const ahead = futureOffsets(nowSec, HORIZON, HORIZON_STEP, curve?.t ?? null);
	x.push(...ahead.map((k) => nowSec + k));
	const pad = (arr: Num[]) => [...arr, ...ahead.map(() => null)];

	const profBtAligned = curve
		? alignToBase(x, curve.t, curve.bt)
		: new Array<Num>(x.length).fill(null);
	const profEtAligned = curve
		? alignToBase(x, curve.t, curve.et)
		: new Array<Num>(x.length).fill(null);
	const rorCurve = curve ? profileRor(curve.t, curve.bt) : null;
	const profRorAligned =
		curve && rorCurve ? alignToBase(x, curve.t, rorCurve) : new Array<Num>(x.length).fill(null);

	// Projections are two linear legs: now → +60 s → +120 s.
	const leg = (from: number, mid: number, to: number, k: number) =>
		k <= HORIZON / 2
			? from + ((mid - from) * k) / (HORIZON / 2)
			: mid + ((to - mid) * (k - HORIZON / 2)) / (HORIZON / 2);
	const projBt: Num[] = new Array(x.length).fill(null);
	const projRor: Num[] = new Array(x.length).fill(null);
	if (pBt) {
		projBt[n - 1] = T(pBt.from[1]);
		ahead.forEach((k, i) => {
			if (k <= HORIZON) projBt[n + i] = T(leg(pBt.from[1], pBt.at60, pBt.to[1], k));
		});
	}
	if (pRor) {
		projRor[n - 1] = D(pRor.from[1]);
		ahead.forEach((k, i) => {
			if (k <= HORIZON) projRor[n + i] = D(leg(pRor.from[1], pRor.at60, pRor.to[1], k));
		});
	}

	// The model's prediction (server, docs/prediction.md): its times are from CHARGE.
	const chargeLm = landmarks.find((l) => l.type === 'charge');
	const live = !dropped && chargeLm ? o.prediction : null;
	const grid = predictionOnGrid(
		live,
		x,
		chargeLm ? landmarkSeconds(chargeLm, sessionStartMs, originMs) : 0
	);

	// Display only (Settings → This browser): the PID's output swings every second
	const heaterLine =
		o.heaterLine === 'smoothed' ? smoothByTime(s.t, s.heater, HEATER_SMOOTH_SEC) : s.heater;
	// Declared channels: °C ones converted like BT, % and other units as sent
	const chLines = channelLines(
		h.map((e) => e.telemetry),
		o.channels,
		(c) => toDisplay(c)
	);

	const data: AlignedData = [
		x,
		pad(s.bt.map(T)),
		pad(s.et.map(T)),
		pad(s.sp.map(T)),
		pad(s.rorBt.map(D)),
		pad(s.rorEt.map(D)),
		pad(heaterLine),
		pad(s.fan),
		profBtAligned.map(T),
		profEtAligned.map(T),
		projBt,
		projRor,
		profBtAligned.map((v, i) => (i < n && s.bt[i] != null && v != null ? T(v) : null)),
		profRorAligned.map(D),
		grid.mid.map(T),
		grid.low.map(T),
		grid.high.map(T),
		...chLines.map((l) => pad(l.values))
	];

	const bands = phaseBands(landmarks, sessionStartMs, originMs, nowSec);
	const marks = landmarks
		.filter((l) => l.type !== 'custom')
		.map((l) => ({
			t: landmarkSeconds(l, sessionStartMs, originMs),
			type: l.type,
			temp: l.temperature ?? null
		}));
	const flags = eventFlags(s);

	const last = h[h.length - 1].telemetry;
	const profNow = curve ? alignToBase([nowSec], curve.t, curve.bt)[0] : null;
	const profRorNow = curve && rorCurve ? alignToBase([nowSec], curve.t, rorCurve)[0] : null;
	// Δ profile only means something once a roast runs.
	const delta =
		!previewing && profNow != null && Number.isFinite(last.beanTemp)
			? last.beanTemp - profNow
			: null;
	// FC target: the profile's FC landmark BT when it has one, else a typical 196 °C.
	const fcEvent = prof?.events?.find(
		(e) => e.action === 'landmark' && e.label === 'first_crack_start'
	);
	const fcTarget = fcEvent?.at_bt ?? 196;
	// Development: target DTR from the profile, else the setting.
	const chargeMark = landmarks.find((l) => l.type === 'charge');
	const fcMark = landmarks.find((l) => l.type === 'first_crack_start');
	const dropMarked = landmarks.some((l) => l.type === 'drop');
	// Tracking: RMS of BT − profile since the turning point (else CHARGE + 60 s).
	const tpMark = landmarks.find((l) => l.type === 'turning_point');
	const trackFrom = tpMark
		? landmarkSeconds(tpMark, sessionStartMs, originMs)
		: (chargeMark ? landmarkSeconds(chargeMark, sessionStartMs, originMs) : 0) + 60;
	let trackN = 0;
	let trackSq = 0;
	if (curve) {
		for (let i = 0; i < n; i++) {
			const b = s.bt[i];
			const p = profBtAligned[i];
			if (s.t[i] < trackFrom || b == null || p == null) continue;
			trackSq += (b - p) * (b - p);
			trackN++;
		}
	}
	const trackRms = trackN >= 10 ? Math.sqrt(trackSq / trackN) : null;
	const dev = dropMarked
		? null
		: developmentGuide({
				chargeSec: chargeMark ? landmarkSeconds(chargeMark, sessionStartMs, originMs) : 0,
				fcSec: fcMark ? landmarkSeconds(fcMark, sessionStartMs, originMs) : null,
				nowSec,
				btNow: Number.isFinite(last.beanTemp) ? last.beanTemp : null,
				btFc: fcMark?.temperature ?? null,
				ror: last.rateOfRise ?? null,
				targetDtr: profileDtr(prof?.target_first_crack, prof?.target_total_time) ?? o.targetDtr,
				targetDropBt: prof?.target_end_temp ?? null
			});
	// Roast guidance: the profile's expected development window, from first crack.
	const devRange = dev ? (expectationsFor(prof ?? null, null).development_s?.range ?? null) : null;
	const fcAt = fcMark ? landmarkSeconds(fcMark, sessionStartMs, originMs) : null;
	const devWindow =
		devRange && fcAt != null
			? { from: fcAt + devRange[0], to: fcAt + devRange[1], lo: devRange[0], hi: devRange[1] }
			: null;
	const xRange = viewRange(o.view, {
		dataMin: s.t[0] ?? nowSec,
		nowSec,
		// A preview frame starts two minutes before "now" (its t = 0).
		sessionStartSec:
			sessionStartMs != null ? (sessionStartMs - originMs) / 1000 : previewing ? -105 : null,
		profileEndSec: curve && curve.t.length ? curve.t[curve.t.length - 1] : null,
		horizon: HORIZON
	});
	return {
		data,
		n,
		nowSec,
		xRange,
		chLines,
		chargeAtOrigin,
		bands,
		pct: phasePercents(bands),
		marks,
		flags,
		ticks,
		stats: {
			bt: last.beanTemp,
			et: last.envTemp,
			ror: last.rateOfRise ?? null,
			rorTarget: profRorNow,
			delta,
			proj60: pBt?.at60 ?? null,
			proj120: pBt?.at120 ?? null,
			prediction: live,
			etaFc: etaToTemp(pBt, fcTarget),
			fcTarget,
			dev,
			devWindow,
			trackRms
		}
	};
}

/** Where the cursor is on the chart: the sample's time and the toggleable series' values there. */
export interface ScopeCursor {
	t: number;
	vals: Partial<Record<SeriesKey, Num>>;
}

/** The header's readout, display units: the cursor's sample while hovering, else now. */
export interface ScopeReadout {
	title: string;
	bt: Num;
	et: Num;
	ror: Num;
	rorTarget: Num;
	delta: Num;
	trackRms: Num;
}

export function scopeReadout(
	m: ScopeModel,
	cursor: ScopeCursor | null,
	previewing: boolean
): ScopeReadout {
	if (cursor) {
		const v = cursor.vals;
		return {
			title: `at ${fmtClock(cursor.t)}`,
			bt: v.bt ?? null,
			et: v.et ?? null,
			ror: v.rorBt ?? null,
			rorTarget: v.profRor ?? null,
			delta: v.bt != null && v.profBt != null ? v.bt - v.profBt : null,
			trackRms: null
		};
	}
	const s = m.stats;
	return {
		title: m.chargeAtOrigin
			? `${fmtClock(m.nowSec)} since charge`
			: previewing
				? 'Preview: profile from now'
				: fmtClock(m.nowSec),
		bt: Number.isFinite(s.bt) ? toDisplay(s.bt) : null,
		et: Number.isFinite(s.et) ? toDisplay(s.et) : null,
		ror: s.ror != null ? deltaToDisplay(s.ror) : null,
		rorTarget: s.rorTarget != null ? deltaToDisplay(s.rorTarget) : null,
		delta: s.delta != null ? deltaToDisplay(s.delta) : null,
		trackRms: s.trackRms != null ? deltaToDisplay(s.trackRms) : null
	};
}

/** The cursor's values at a sample: null past the live samples. */
export function cursorAt(m: ScopeModel, idx: number | null | undefined): ScopeCursor | null {
	if (idx == null || idx < 0 || idx >= m.n) return null;
	const d = m.data;
	const vals: Partial<Record<SeriesKey, Num>> = {};
	for (const k of TOGGLEABLE) vals[k] = (d[SERIES[k]][idx] as Num) ?? null;
	return { t: d[0][idx] as number, vals };
}
