/**
 * The roast summary (docs/roast-guidance.md): at most three short lines naming only what is
 * off, in this order: colour, weight loss, development, first crack against the profile, total
 * time, RoR events, temperatures against own history. Nothing off, no lines.
 */
import type { RoastEvent } from '#lib/api/events.js';
import type { RoastSession } from '#lib/api/sessions.js';
import type { SessionTelemetryPoint } from '#lib/api/sessions.js';
import { roastSummary as reportSummary, rorSeries, type Summary } from '#lib/roast/report.js';
import {
	expectationsFor,
	historyNote,
	type Expectation,
	type Expectations,
	type HistoryRoast,
	type ProfileLike
} from './expect.js';
import { judge, isOutside, type Judgement } from './judge.js';
import { levelForLoss, levelLabel, toAgtron, LEVELS } from './references.js';
import { rorEvents, rorEventText, type RorEvent } from './ror.js';

/** What one roast showed. Times are seconds from CHARGE, temperatures °C. */
export interface RoastFacts {
	fc_s: number | null;
	fc_bt: number | null;
	drop_s: number | null;
	drop_bt: number | null;
	dry_bt: number | null;
	development_s: number | null;
	/** % */
	dtr_pct: number | null;
	total_s: number | null;
	weight_loss_pct: number | null;
	moisture_green: number | null;
	color_ground: number | null;
	color_scale: string | null;
}

export function factsFrom(session: RoastSession, sum: Summary): RoastFacts {
	const loss =
		session.weight_loss_pct ??
		(session.green_weight && session.roasted_weight
			? ((session.green_weight - session.roasted_weight) / session.green_weight) * 100
			: null);
	const dev = sum.fc && sum.drop ? sum.drop.t - sum.fc.t : null;
	return {
		fc_s: sum.fc?.t ?? null,
		fc_bt: sum.fc?.temp ?? null,
		drop_s: sum.drop?.t ?? null,
		drop_bt: sum.drop?.temp ?? null,
		dry_bt: sum.dry?.temp ?? null,
		development_s: dev,
		dtr_pct: dev != null && sum.drop && sum.drop.t > 0 ? (dev / sum.drop.t) * 100 : sum.dtr,
		total_s: sum.drop?.t ?? null,
		weight_loss_pct: loss,
		moisture_green: session.moisture_green ?? null,
		color_ground: session.color_ground ?? null,
		color_scale: session.color_scale ?? null
	};
}

export interface Judgements {
	color_ground: Judgement | null;
	weight_loss_pct: Judgement | null;
	development_s: Judgement | null;
	dtr_pct: Judgement | null;
	total_s: Judgement | null;
	fc_bt: Judgement | null;
	drop_bt: Judgement | null;
	fc_s: Judgement | null;
}

/** Each measure against its expected range; the colour converted to ground Agtron first. */
export function judgeRoast(f: RoastFacts, exp: Expectations): Judgements {
	return {
		color_ground: judge(toAgtron(f.color_ground, f.color_scale).agtron, exp.color_ground),
		weight_loss_pct: judge(f.weight_loss_pct, exp.weight_loss_pct),
		development_s: judge(f.development_s, exp.development_s),
		dtr_pct: judge(f.dtr_pct, exp.dtr_pct),
		total_s: judge(f.total_s, exp.total_s),
		fc_bt: judge(f.fc_bt, exp.fc_bt),
		drop_bt: judge(f.drop_bt, exp.drop_bt),
		fc_s: judge(f.fc_s, exp.fc_s)
	};
}

/** First crack this far from the profile's is worth a line (FC marking error is 20–40 s). */
const FC_TOLERANCE_S = 30;

interface Formatters {
	/** °C → display text with unit. */
	temp: (c: number) => string;
	/** °C/min → display text with unit. */
	rate: (c: number) => string;
}

const CELSIUS: Formatters = {
	temp: (c) => `${(Math.round(c * 10) / 10).toFixed(1)} °C`,
	rate: (c) => `${Math.round(c)} °C/min`
};

export interface SummaryInput {
	facts: RoastFacts;
	exp: Expectations;
	/** The profile's first crack, s from CHARGE. */
	profileFc?: number | null;
	ror?: RorEvent[];
	fmt?: Partial<Formatters>;
}

/** Up to three lines, most important first; empty when nothing is off. */
export function guidanceSummary({
	facts,
	exp,
	profileFc,
	ror = [],
	fmt: f = {}
}: SummaryInput): string[] {
	const fmt = { ...CELSIUS, ...f };
	const j = judgeRoast(facts, exp);
	const lines: string[] = [];
	const who = (e: Expectation) => expects(e, exp);

	if (isOutside(j.color_ground)) {
		const c = j.color_ground!;
		const reading = toAgtron(facts.color_ground, facts.color_scale);
		const shown =
			facts.color_scale && facts.color_scale !== 'agtron'
				? `${facts.color_ground} (about Agtron ${Math.round(reading.agtron!)})`
				: `Agtron ${num(facts.color_ground!)}`;
		const lighter = c.state === 'above';
		const range =
			c.exp.source === 'default' ? bandText(c.range) : `${num(c.range[0])}–${num(c.range[1])}`;
		lines.push(
			`Ground colour ${shown}: ${who(c.exp)} ${range}, so it is ${lighter ? 'lighter' : 'darker'} (${levelOf(reading.agtron!)}).`
		);
	}

	if (isOutside(j.weight_loss_pct)) {
		const w = j.weight_loss_pct!;
		const implied = levelForLoss(facts.weight_loss_pct!, facts.moisture_green);
		let line = `Weight loss ${num(facts.weight_loss_pct!, 1)} %: ${who(w.exp)} ${num(w.range[0])}–${num(w.range[1])} %.`;
		if (implied.level !== exp.level) {
			line += ` Loss like this usually means ${article(levelLabel(implied.level).toLowerCase())} roast (about Agtron ${implied.agtron[0]}–${implied.agtron[1]})`;
			line +=
				j.color_ground && j.color_ground.state !== 'waiting'
					? '.'
					: '; a colour reading would confirm it.';
		}
		lines.push(line);
	}

	if (isOutside(j.development_s)) {
		const d = j.development_s!;
		const dtr = facts.dtr_pct != null ? ` (DTR ${Math.round(facts.dtr_pct)} %)` : '';
		const edit = d.state === 'above' ? 'Shorten' : 'Lengthen';
		lines.push(
			`Development ${mmss(facts.development_s!)}${dtr}: ${who(d.exp)} ${mmss(d.range[0])}–${mmss(d.range[1])}. ${edit} the profile after first crack.`
		);
	}

	if (
		facts.fc_s != null &&
		profileFc != null &&
		Math.abs(facts.fc_s - profileFc) >= FC_TOLERANCE_S
	) {
		const diff = facts.fc_s - profileFc;
		lines.push(
			`First crack ${mmss(Math.abs(diff))} ${diff < 0 ? 'earlier' : 'later'} than the profile (${mmss(facts.fc_s)} vs ${mmss(profileFc)}).`
		);
	}

	if (isOutside(j.total_s)) {
		const t = j.total_s!;
		lines.push(
			`Total time ${mmss(facts.total_s!)}: ${who(t.exp)} ${mmss(t.range[0])}–${mmss(t.range[1])}.`
		);
	}

	for (const e of ror) lines.push(rorEventText(e, fmt.rate));

	for (const [key, name] of [
		['fc_bt', 'First crack'],
		['drop_bt', 'Drop']
	] as const) {
		const t = j[key];
		if (!isOutside(t)) continue;
		lines.push(
			`${name} at ${fmt.temp(t!.value!)}: ${who(t!.exp)} ${fmt.temp(t!.range[0]).replace(/ °[CF]$/, '')}–${fmt.temp(t!.range[1])}.`
		);
	}

	return lines.slice(0, 3);
}

/** "Light expects", "the profile expects", "this profile usually runs (4 roasts)". */
function expects(e: Expectation, exp: Expectations): string {
	if (e.source === 'profile') return 'the profile expects';
	if (e.source === 'history') return `this profile’s last ${e.n} roasts here ran`;
	return `${levelLabel(exp.level)} expects`;
}

function bandText([lo, hi]: [number, number]): string {
	if (lo <= 0) return `below ${hi}`;
	if (hi >= 150) return `${lo} or more`;
	return `${lo}–${hi}`;
}

function levelOf(agtron: number): string {
	return (LEVELS.find((l) => agtron >= l.agtron[0])?.label ?? 'Dark').toLowerCase();
}

function article(word: string): string {
	return /^[aeiou]/.test(word) ? `an ${word}` : `a ${word}`;
}

function num(x: number, decimals = 0): string {
	const r = Number(x.toFixed(decimals || 1));
	return decimals ? r.toFixed(decimals) : String(r);
}

export function mmss(s: number): string {
	const a = Math.round(Math.abs(s));
	return `${Math.floor(a / 60)}:${String(a % 60).padStart(2, '0')}`;
}

export interface RoastReading {
	exp: Expectations;
	facts: RoastFacts;
	j: Judgements;
	lines: string[];
	/** A muted line under the summary: no level on the profile, or history too thin. */
	note: string | null;
	/** The level the weight loss suggests, for the loss hint. */
	lossSuggests: string | null;
}

export interface ReadInput {
	session: RoastSession & {
		profile?: (ProfileLike & { target_first_crack?: number | null }) | null;
	};
	events: RoastEvent[];
	telemetry: SessionTelemetryPoint[];
	/** `GET /api/profiles/{id}/history` for the session's profile and roaster. */
	history?: HistoryRoast[];
	fmt?: Partial<Formatters>;
}

/** Everything a page shows about one finished roast. */
export function readRoast({
	session,
	events,
	telemetry,
	history = [],
	fmt
}: ReadInput): RoastReading {
	const sum = reportSummary(events, telemetry);
	const facts = factsFrom(session, sum);
	const exp = expectationsFor(session.profile ?? null, session, history);
	const ror = rorEvents(rorSeries(telemetry, sum.chargeAt), sum.fc?.t, sum.drop?.t);
	const lines = guidanceSummary({
		facts,
		exp,
		profileFc: session.profile?.target_first_crack ?? null,
		ror,
		fmt
	});
	const notes: string[] = [];
	if (!exp.level && session.profile && !session.profile.expectations)
		notes.push(
			'Set a roast level on the profile to read weight loss, colour and development against references.'
		);
	const thin = session.profile ? historyNote(exp.historyCount) : null;
	if (thin && (facts.fc_bt != null || facts.drop_bt != null)) notes.push(thin);
	let lossSuggests: string | null = null;
	if (facts.weight_loss_pct != null) {
		const l = levelForLoss(facts.weight_loss_pct, facts.moisture_green);
		lossSuggests = `A loss of ${num(facts.weight_loss_pct, 1)} % usually means ${article(levelLabel(l.level).toLowerCase())} roast (about Agtron ${l.agtron[0]}–${l.agtron[1]}).`;
	}
	return {
		exp,
		facts,
		j: judgeRoast(facts, exp),
		lines,
		note: notes.join(' ') || null,
		lossSuggests
	};
}

/**
 * Which History table columns are outside their expected range (an amber dot after the
 * value): loss, ground colour, and development (judged in seconds from Time × DTR, shown on
 * the DTR column). No history here: the table judges only against the level.
 */
export function outsideColumns(
	s: RoastSession,
	profile: ProfileLike | null | undefined
): Set<'loss' | 'colorGround' | 'dtr'> {
	const exp = expectationsFor(profile ?? null, s);
	const out = new Set<'loss' | 'colorGround' | 'dtr'>();
	if (isOutside(judge(s.weight_loss_pct, exp.weight_loss_pct))) out.add('loss');
	if (isOutside(judge(toAgtron(s.color_ground, s.color_scale).agtron, exp.color_ground)))
		out.add('colorGround');
	const dev =
		s.total_time_seconds != null && s.development_time_ratio != null
			? s.total_time_seconds * s.development_time_ratio
			: null;
	if (isOutside(judge(dev, exp.development_s))) out.add('dtr');
	return out;
}
