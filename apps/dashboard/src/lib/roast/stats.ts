/**
 * Roast statistics (docs/statistics.md): AUC per phase, the RoR slope from
 * first crack to drop, and consistency bands over several roasts. Times are roast time
 * (s from CHARGE), temperatures °C, AUC °C·min.
 */
import type { SessionTelemetryPoint } from '#lib/api/sessions.js';
import { interp } from './compare.js';
import { rorSeries, type Summary } from './report.js';

/** Where the AUC starts; the server's `auc_start_event` setting. A missing mark means CHARGE. */
export type AucStart = 'charge' | 'turning_point' | 'drying_end' | 'first_crack_start';

export interface AucConfig {
	/** °C; only BT above it counts. The server's `auc_base_temp` (default 0). */
	base: number;
	start: AucStart;
}

export const DEFAULT_AUC: AucConfig = { base: 0, start: 'charge' };

/** The AUC settings from `/api/settings`. */
export function aucConfig(settings: Record<string, string> | null | undefined): AucConfig {
	const base = parseFloat(settings?.auc_base_temp ?? '');
	const start = settings?.auc_start_event as AucStart | undefined;
	return {
		base: Number.isFinite(base) ? base : 0,
		start:
			start && ['charge', 'turning_point', 'drying_end', 'first_crack_start'].includes(start)
				? start
				: 'charge'
	};
}

/** BT as roast-time points, sorted. */
function btPoints(telemetry: SessionTelemetryPoint[], chargeAt: number): [number, number][] {
	return telemetry
		.filter((p) => p.bean_temp != null)
		.map((p) => [p.elapsed_seconds - chargeAt, p.bean_temp as number]);
}

/**
 * Area of BT above `base` between `from` and `to` (trapezoids, the ends interpolated), in
 * °C·min. Only where there are samples.
 */
export function aucBetween(
	pts: [number, number][],
	from: number,
	to: number,
	base: number
): number {
	if (pts.length < 2 || to <= from) return 0;
	const a = Math.max(from, pts[0][0]);
	const b = Math.min(to, pts[pts.length - 1][0]);
	if (b <= a) return 0;
	const inner = pts.filter(([t]) => t > a && t < b);
	const seq: [number, number][] = [[a, interp(pts, a)!], ...inner, [b, interp(pts, b)!]];
	let sum = 0;
	for (let i = 1; i < seq.length; i++) {
		const [t0, v0] = seq[i - 1];
		const [t1, v1] = seq[i];
		const d0 = v0 - base;
		const d1 = v1 - base;
		if (d0 >= 0 && d1 >= 0) sum += ((t1 - t0) * (d0 + d1)) / 2;
		else if (d0 > 0 || d1 > 0) {
			// Crosses the base inside the segment: only the part above it counts.
			const up = Math.max(d0, d1);
			sum += ((t1 - t0) * (up / (up - Math.min(d0, d1))) * up) / 2;
		}
	}
	return sum / 60;
}

/** Roast time where the AUC starts. */
function aucStartAt(sum: Summary, start: AucStart): number {
	const at = {
		charge: 0,
		turning_point: sum.tp?.t,
		drying_end: sum.dry?.t,
		first_crack_start: sum.fc?.t
	}[start];
	return at ?? 0;
}

export interface PhaseAuc {
	drying: number | null;
	maillard: number | null;
	development: number | null;
	/** Start → DROP (else the last sample). */
	total: number | null;
}

/** AUC per phase (CHARGE→DRY, DRY→FC, FC→DROP), each clipped to the AUC start. */
export function phaseAuc(
	sum: Summary,
	telemetry: SessionTelemetryPoint[],
	cfg: AucConfig = DEFAULT_AUC
): PhaseAuc {
	const pts = btPoints(telemetry, sum.chargeAt);
	const end = sum.drop?.t ?? sum.total;
	if (pts.length < 2 || end == null)
		return { drying: null, maillard: null, development: null, total: null };
	const start = aucStartAt(sum, cfg.start);
	const span = (a: number | null | undefined, b: number | null | undefined) =>
		a == null || b == null || b <= Math.max(a, start)
			? null
			: aucBetween(pts, Math.max(a, start), b, cfg.base);
	return {
		drying: span(0, sum.dry?.t),
		maillard: span(sum.dry?.t, sum.fc?.t),
		development: span(sum.fc?.t, end),
		total: aucBetween(pts, start, end, cfg.base)
	};
}

/** Least-squares slope of y over x (per second); null with fewer than 3 points. */
export function slope(pts: [number, number][]): number | null {
	const n = pts.length;
	if (n < 3) return null;
	const mx = pts.reduce((a, [x]) => a + x, 0) / n;
	const my = pts.reduce((a, [, y]) => a + y, 0) / n;
	let sxy = 0;
	let sxx = 0;
	for (const [x, y] of pts) {
		sxy += (x - mx) * (y - my);
		sxx += (x - mx) ** 2;
	}
	return sxx > 0 ? sxy / sxx : null;
}

/**
 * How fast RoR falls from first crack to drop, °C/min per minute (negative = falling).
 * The RoR is the report's ±15 s slope; it stops 15 s before DROP so the dump does not count.
 * Null without FC and DROP 45 s apart.
 */
export function fcDropRorSlope(sum: Summary, telemetry: SessionTelemetryPoint[]): number | null {
	if (!sum.fc || !sum.drop) return null;
	const from = sum.fc.t;
	const to = sum.drop.t - 15;
	if (to - from < 30) return null;
	const ror = rorSeries(telemetry, sum.chargeAt).filter(([t]) => t >= from && t <= to);
	const s = slope(ror);
	return s == null ? null : s * 60;
}

export interface BandPoint {
	t: number;
	mean: number;
	/** Sample standard deviation (n − 1). */
	sd: number;
	n: number;
}

/**
 * Mean ± σ of several aligned curves every `step` s, where at least `minN` of them have a
 * value.
 */
export function consistencyBand(
	curves: [number, number][][],
	from: number,
	to: number,
	step = 5,
	minN = 2
): BandPoint[] {
	const out: BandPoint[] = [];
	for (let t = Math.ceil(from / step) * step; t <= to; t += step) {
		const vs = curves.map((c) => interp(c, t)).filter((v): v is number => v != null);
		if (vs.length < Math.max(2, minN)) continue;
		const mean = vs.reduce((a, b) => a + b, 0) / vs.length;
		const sd = Math.sqrt(vs.reduce((a, b) => a + (b - mean) ** 2, 0) / (vs.length - 1));
		out.push({ t, mean, sd, n: vs.length });
	}
	return out;
}

/** Mean and sample σ of the numbers given (nulls skipped); null below two. */
export function meanSd(
	values: (number | null | undefined)[]
): { mean: number; sd: number; n: number } | null {
	const vs = values.filter((v): v is number => v != null && Number.isFinite(v));
	if (vs.length < 2) return null;
	const mean = vs.reduce((a, b) => a + b, 0) / vs.length;
	const sd = Math.sqrt(vs.reduce((a, b) => a + (b - mean) ** 2, 0) / (vs.length - 1));
	return { mean, sd, n: vs.length };
}

export interface LiveAuc {
	/** The roast so far, start → now. */
	now: number;
	/** The profile's, start → the same roast time. */
	profileNow: number | null;
	/** The profile's, start → its end: the target. */
	target: number | null;
}

/**
 * Live AUC against the profile: `pts` is the roast so far and `profile` the profile's BT, both
 * in roast time; `startAt` where the AUC starts on each (the profile's from its own marks).
 */
export function liveAuc(
	pts: [number, number][],
	profile: [number, number][] | null,
	cfg: AucConfig,
	startAt = 0,
	profileStartAt = startAt
): LiveAuc | null {
	if (pts.length < 2) return null;
	const nowT = pts[pts.length - 1][0];
	const now = aucBetween(pts, startAt, nowT, cfg.base);
	if (!profile || profile.length < 2) return { now, profileNow: null, target: null };
	const end = profile[profile.length - 1][0];
	return {
		now,
		profileNow: aucBetween(profile, profileStartAt, Math.min(nowT, end), cfg.base),
		target: aucBetween(profile, profileStartAt, end, cfg.base)
	};
}
