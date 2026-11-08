/**
 * Guidance for a profile itself (docs/roast-guidance.md, "Profiles"): what its curve plans
 * against its roast level, what results the level should give, and how its roasts on this
 * roaster actually ran. Times are seconds from CHARGE, temperatures °C.
 */
import {
	expectationsFor,
	type Expectations,
	type HistoryRoast,
	type ProfileLike
} from './expect.js';
import { judge, isOutside, type Judgement } from './judge.js';
import { bandText, levelLabel, refBasis, toAgtron } from './references.js';
import { mmss } from './summary.js';

/** What a profile's curve plans. */
export interface Plan {
	fc_s: number | null;
	/** The profile's first-crack BT (its FC landmark, else the curve there). */
	fc_bt: number | null;
	total_s: number | null;
	/** Drying, Maillard and development as % of the roast; null where a mark is missing. */
	phases: { drying: number | null; maillard: number | null; development: number | null };
}

function planDevelopment(p: Plan): number | null {
	return p.fc_s != null && p.total_s != null && p.total_s > p.fc_s ? p.total_s - p.fc_s : null;
}

export interface PlanReading {
	exp: Expectations;
	development_s: number | null;
	dtr_pct: number | null;
	/** The planned development against the level (null without a level or a first crack). */
	development: Judgement | null;
	dtr: Judgement | null;
	/** "Light: weight loss 14–16 %, ground Agtron 70–80, development 1:00–2:00." */
	expected: string | null;
	/** "Drying 39 % · Maillard 39 % · development 22 %", for information only. */
	phases: string | null;
}

/** The curve against the level: judged on development; the results it should give. */
export function readPlan(plan: Plan, profile: ProfileLike | null | undefined): PlanReading {
	const exp = expectationsFor(profile ?? null, null);
	const development_s = planDevelopment(plan);
	const dtr_pct =
		development_s != null && plan.total_s ? (development_s / plan.total_s) * 100 : null;
	let expected: string | null = null;
	if (exp.level) {
		const parts: string[] = [];
		if (exp.weight_loss_pct)
			parts.push(
				`weight loss ${num(exp.weight_loss_pct.range[0])}–${num(exp.weight_loss_pct.range[1])} %`
			);
		if (exp.color_ground)
			parts.push(
				`ground Agtron ${exp.color_ground.source === 'default' ? bandText(exp.color_ground.range) : `${num(exp.color_ground.range[0])}–${num(exp.color_ground.range[1])}`}`
			);
		if (exp.development_s)
			parts.push(
				`development ${mmss(exp.development_s.range[0])}–${mmss(exp.development_s.range[1])}`
			);
		expected = `${levelLabel(exp.level)}: ${parts.join(', ')}.`;
	}
	const ph = plan.phases;
	const phases =
		ph.drying != null && ph.maillard != null && ph.development != null
			? `Drying ${Math.round(ph.drying)} % · Maillard ${Math.round(ph.maillard)} % · development ${Math.round(ph.development)} %`
			: null;
	return {
		exp,
		development_s,
		dtr_pct,
		development: development_s != null ? judge(development_s, exp.development_s) : null,
		dtr: dtr_pct != null ? judge(dtr_pct, exp.dtr_pct) : null,
		expected,
		phases
	};
}

/** Hint text for the phase split: community practice, never judged. */
export const PHASE_HINT = refBasis('phase_split');

// ---- the roasts made on a profile ----

/** First crack this far from the profile's mark (°C) is worth moving the mark. */
const FC_TEMP_TOLERANCE = 3;
/** First crack this far from the profile's time is worth a line (FC marking error is 20–40 s). */
const FC_TIME_TOLERANCE_S = 30;
/** Roasts the review looks at, newest first. */
const REVIEW_ROASTS = 5;

interface ReviewRow {
	roast: HistoryRoast;
	/** First crack minus the profile's, s. */
	fcDelta: number | null;
	development: Judgement | null;
	loss: Judgement | null;
	colour: Judgement | null;
}

export interface ProfileReview {
	rows: ReviewRow[];
	/** At most three lines naming what the profile's roasts keep doing differently. */
	lines: string[];
	/** Roasts the lines average over. */
	n: number;
}

const mean = (xs: number[]) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);

/**
 * How the profile's roasts ran (newest first), and what to edit: first crack earlier or later
 * or at another temperature than the profile marks, development outside the level, loss or
 * colour outside the level. Lines average the newest `REVIEW_ROASTS` roasts and say how many.
 */
export function reviewProfile(
	plan: Plan,
	profile: ProfileLike | null | undefined,
	history: HistoryRoast[],
	fmtTemp: (c: number) => string = (c) => `${num(c, 1)} °C`
): ProfileReview {
	const exp = expectationsFor(profile ?? null, null);
	const rows: ReviewRow[] = history.map((h) => ({
		roast: h,
		fcDelta: h.fc_s != null && plan.fc_s != null ? h.fc_s - plan.fc_s : null,
		development: judge(h.development_s, exp.development_s),
		loss: judge(h.weight_loss_pct, exp.weight_loss_pct),
		colour: judge(toAgtron(h.color_ground, h.color_scale).agtron, exp.color_ground)
	}));
	const recent = history.slice(0, REVIEW_ROASTS);
	const n = recent.length;
	const roasts = (k: number) => (k === 1 ? '1 roast' : `${k} roasts`);
	const lines: string[] = [];
	const level = levelLabel(exp.level);

	// First crack: time and temperature against the profile's mark.
	const fcS = recent.map((h) => h.fc_s).filter((x): x is number => x != null);
	const fcT = recent.map((h) => h.fc_bt).filter((x): x is number => x != null);
	const dt = mean(fcS) != null && plan.fc_s != null ? mean(fcS)! - plan.fc_s : null;
	const dT = mean(fcT) != null && plan.fc_bt != null ? mean(fcT)! - plan.fc_bt : null;
	const timeOff = dt != null && Math.abs(dt) >= FC_TIME_TOLERANCE_S;
	const tempOff = dT != null && Math.abs(dT) >= FC_TEMP_TOLERANCE;
	if (timeOff || tempOff) {
		const parts: string[] = [];
		if (timeOff)
			parts.push(
				`${mmss(Math.abs(dt!))} ${dt! < 0 ? 'earlier' : 'later'} than the profile (${mmss(mean(fcS)!)} vs ${mmss(plan.fc_s!)})`
			);
		if (tempOff)
			parts.push(
				`at ${fmtTemp(mean(fcT)!)}, ${num(Math.abs(dT!), 1)} °C ${dT! < 0 ? 'below' : 'above'} its mark`
			);
		let line = `First crack came ${parts.join(' and ')} (${roasts(timeOff ? fcS.length : fcT.length)}).`;
		if (tempOff)
			line += ` Set the profile’s FC mark to ${fmtTemp(Math.round(mean(fcT)!)).replace(/\.0(?=\D*$)/, '')} so the scope’s countdown matches this probe.`;
		lines.push(line);
	}

	// Development against the level, and what the profile itself plans.
	const devs = recent.map((h) => h.development_s).filter((x): x is number => x != null);
	const dev = mean(devs);
	const devJ = judge(dev, exp.development_s);
	if (isOutside(devJ)) {
		const planned = planDevelopment(plan);
		const plannedJ = judge(planned, exp.development_s);
		let line = `Development ran ${mmss(dev!)} (${roasts(devs.length)}); ${level} expects ${mmss(devJ!.range[0])}–${mmss(devJ!.range[1])}.`;
		if (planned != null && isOutside(plannedJ))
			line += ` The profile itself plans ${mmss(planned)}: ${plannedJ!.state === 'above' ? 'shorten' : 'lengthen'} it after first crack.`;
		else if (planned != null)
			line += ` The profile plans ${mmss(planned)}: drop ${devJ!.state === 'above' ? 'sooner' : 'later'} after first crack.`;
		lines.push(line);
	}

	// Results against the level.
	const losses = recent.map((h) => h.weight_loss_pct).filter((x): x is number => x != null);
	const lossJ = judge(mean(losses), exp.weight_loss_pct);
	if (isOutside(lossJ))
		lines.push(
			`Weight loss averaged ${num(mean(losses)!, 1)} % (${roasts(losses.length)}); ${level} expects ${num(lossJ!.range[0])}–${num(lossJ!.range[1])} %.`
		);
	const colours = recent
		.map((h) => toAgtron(h.color_ground, h.color_scale).agtron)
		.filter((x): x is number => x != null);
	const colourJ = judge(mean(colours), exp.color_ground);
	if (isOutside(colourJ))
		lines.push(
			`Ground colour averaged Agtron ${Math.round(mean(colours)!)} (${roasts(colours.length)}); ${level} expects ${bandText(colourJ!.range)}.`
		);

	return { rows, lines: lines.slice(0, 3), n };
}

/** A Plan from the profile page's view (`lib/profile/view.ts`). */
export function planFromView(v: {
	landmarks: { code: string; t: number; bt: number | null }[];
	phases: { key: 'drying' | 'maillard' | 'development'; pct: number | null }[];
	total: number | null;
}): Plan {
	const fc = v.landmarks.find((l) => l.code === 'FCs');
	const pct = (k: string) => v.phases.find((p) => p.key === k)?.pct ?? null;
	return {
		fc_s: fc?.t ?? null,
		fc_bt: fc?.bt ?? null,
		total_s: v.total,
		phases: { drying: pct('drying'), maillard: pct('maillard'), development: pct('development') }
	};
}

/** A Plan from the designer's points: first crack and dry end (landmark time, else the BT
 * threshold) and the last point. */
export function planFromCurve(o: {
	fc_s: number | null;
	fc_bt: number | null;
	dry_s: number | null;
	total_s: number | null;
}): Plan {
	const { fc_s, dry_s, total_s } = o;
	const share = (a: number | null, b: number | null) =>
		a != null && b != null && total_s && b > a ? ((b - a) / total_s) * 100 : null;
	return {
		fc_s,
		fc_bt: o.fc_bt,
		total_s,
		phases: {
			drying: share(0, dry_s),
			maillard: share(dry_s, fc_s),
			development: share(fc_s, total_s)
		}
	};
}

function num(x: number, decimals = 0): string {
	const r = Number(x.toFixed(decimals || 1));
	return decimals ? r.toFixed(decimals) : String(r);
}
