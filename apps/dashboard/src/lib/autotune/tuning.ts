import type { Gains, GainSchedule, ScheduleRow } from '#lib/api/tuning.js';

/** Firmware warning codes in plain words. */
const WARNINGS: Record<string, string> = {
	retargeted: 'The heater could not reach the target, so the test ran lower.',
	asymmetric: 'Heating and cooling half-cycles were uneven.',
	period_spread: 'Cycle lengths varied between cycles.',
	small_amplitude: 'The temperature swing was small, so readings are coarse.',
	gain_clamped: 'A gain hit its limit and was clamped.',
	not_converged: 'The cycles never settled; results average the last ones.'
};

export function warningText(code: string): string {
	return WARNINGS[code] ?? code.replace(/_/g, ' ');
}

export type Tone = 'good' | 'warn' | 'bad' | 'info' | 'muted';

export const toneText: Record<Tone, string> = {
	good: 'text-green-400',
	warn: 'text-amber-400',
	bad: 'text-red-400',
	info: 'text-blue-400',
	muted: 'text-muted-foreground'
};

export const toneBadge: Record<Tone, string> = {
	good: 'bg-green-500/20 text-green-400 border-green-500/30',
	warn: 'bg-amber-500/20 text-amber-400 border-amber-500/30',
	bad: 'bg-red-500/20 text-red-400 border-red-500/30',
	info: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
	muted: 'bg-muted text-muted-foreground border-border'
};

export function qualityInfo(
	q: string | null | undefined,
	state?: string | null
): { label: string; tone: Tone } {
	if (state && state !== 'complete') return { label: 'Failed', tone: 'bad' };
	switch (q) {
		case 'good':
			return { label: 'Good', tone: 'good' };
		case 'acceptable':
			return { label: 'Acceptable', tone: 'warn' };
		case 'poor':
			return { label: 'Poor', tone: 'bad' };
		default:
			return { label: q ? q : 'Unrated', tone: 'muted' };
	}
}

const RULE_LABELS: Record<string, string> = {
	simc_pi: 'SIMC PI',
	amigo_pi: 'AMIGO PI',
	tyreus_luyben_pi: 'Tyreus–Luyben PI',
	zn_pi: 'Ziegler–Nichols PI',
	tyreus_luyben: 'Tyreus–Luyben PID',
	zn_no_overshoot: 'Z–N no overshoot',
	zn_some_overshoot: 'Z–N some overshoot',
	zn_classic: 'Z–N classic'
};

export function ruleLabel(id: string | null | undefined): string {
	return id ? (RULE_LABELS[id] ?? id) : '';
}

export function sourceLabel(source: string): string {
	switch (source) {
		case 'manual':
			return 'Manual';
		case 'autotune':
			return 'Autotune';
		case 'autotune_apply':
			return 'Autotune (device rule)';
		case 'undo':
			return 'Undo';
		default:
			return source;
	}
}

/** Gains shown to the precision they are tuned at. */
export function fmtGains(g: Gains | null | undefined): { kp: string; ki: string; kd: string } {
	if (!g) return { kp: '—', ki: '—', kd: '—' };
	return { kp: g.kp.toFixed(2), ki: g.ki.toFixed(4), kd: g.kd.toFixed(2) };
}

/** Same as the server: linear between rows, flat beyond the ends; null when off or empty. */
export function scheduleGainsAt(
	s: GainSchedule | null | undefined,
	fanPct: number | null | undefined
): Gains | null {
	if (!s || !s.enabled || fanPct == null || !Number.isFinite(fanPct)) return null;
	const rows = [...s.rows].sort((a, b) => a.fan_pct - b.fan_pct);
	if (rows.length === 0) return null;
	const first = rows[0];
	const last = rows[rows.length - 1];
	const pick = (r: Gains): Gains => ({ kp: r.kp, ki: r.ki, kd: r.kd });
	if (fanPct <= first.fan_pct) return pick(first);
	if (fanPct >= last.fan_pct) return pick(last);
	const i = rows.findIndex((r) => r.fan_pct >= fanPct);
	const a = rows[i - 1];
	const b = rows[i];
	const f = (fanPct - a.fan_pct) / (b.fan_pct - a.fan_pct);
	const lerp = (x: number, y: number) => x + (y - x) * f;
	return { kp: lerp(a.kp, b.kp), ki: lerp(a.ki, b.ki), kd: lerp(a.kd, b.kd) };
}

/** "API error 409: cannot apply: …" → "cannot apply: …". */
export function errorText(e: unknown): string {
	const msg = e instanceof Error ? e.message : String(e);
	return msg.replace(/^API error \d+:\s*/, '');
}

/** A number box's value: an emptied `type=number` input binds null (or ''), not 0. */
function given(v: unknown): v is number {
	return typeof v === 'number' && Number.isFinite(v);
}

/** Why the schedule cannot be saved as it stands, or null. An empty Kd is fine: it is 0. */
export function scheduleProblem(rows: ScheduleRow[]): string | null {
	for (const r of rows) {
		if (!given(r.fan_pct)) return 'Each row needs its fan %.';
		const at = `at ${r.fan_pct} % fan`;
		if (!given(r.kp)) return `Kp is empty ${at}.`;
		if (!given(r.ki)) return `Ki is empty ${at} (0 turns the integral off).`;
		if ([r.kp, r.ki, r.kd].some((v) => given(v) && v < 0)) return `Gains ${at} cannot be negative.`;
	}
	return null;
}

/** The rows as the server takes them: sorted by fan, an empty Kd sent as 0. */
export function cleanRows(rows: ScheduleRow[]): ScheduleRow[] {
	return rows
		.map((r) => ({ ...r, kd: given(r.kd) ? r.kd : 0 }))
		.sort((a, b) => a.fan_pct - b.fan_pct);
}

// --- Autotune 2 (docs/autotune-2.md) ---

/** What the server's test is doing, in words. */
export function tunePhaseInfo(phase: string | null | undefined): { label: string; tone: Tone } {
	switch (phase) {
		case 'approach':
			return { label: 'Heating to the test level', tone: 'info' };
		case 'symmetric':
			return { label: 'Finding the holding power', tone: 'warn' };
		case 'asymmetric':
			return { label: 'Measuring the response', tone: 'warn' };
		case 'analysing':
			return { label: 'Analysing', tone: 'info' };
		case 'done':
			return { label: 'Complete', tone: 'good' };
		case 'stopped':
			return { label: 'Stopped', tone: 'muted' };
		case 'failed':
			return { label: 'Failed', tone: 'bad' };
		default:
			return { label: phase ?? '', tone: 'muted' };
	}
}

/** SIMC's knob (τc as a multiple of the dead time) in words. */
export function tauCLabel(tc: number): string {
	if (tc < 1.25) return 'tight';
	if (tc < 1.75) return 'balanced';
	if (tc < 2.5) return 'calm';
	return 'smooth';
}

/** Ms in words (Åström & Hägglund: 1.2–1.4 robust, up to 1.7 balanced, above 2 fragile). */
export function msInfo(ms: number): { label: string; tone: Tone } {
	if (ms <= 1.45) return { label: 'robust', tone: 'good' };
	if (ms <= 1.8) return { label: 'balanced', tone: 'good' };
	if (ms <= 2.0) return { label: 'little margin', tone: 'warn' };
	return { label: 'fragile', tone: 'bad' };
}

/** Process-unit PI (% per °C) in the roaster's units (0–255 per °C). */
export function toDeviceGains(pid: { kp: number; ki: number }): Gains {
	return { kp: pid.kp * 2.55, ki: pid.ki * 2.55, kd: 0 };
}

/** A model in one line: what the roaster acts like near the test level. */
export function describeModel(
	m: { k: number; t1: number; t2: number; l: number; integrating?: boolean } | null | undefined
): string {
	if (!m) return '';
	const kv = m.integrating ? m.k : m.k / m.t1;
	const rate = `${(kv * 60).toFixed(2)} °C/min per % heater`;
	const delay = `${m.l.toFixed(1)} s dead time`;
	if (m.integrating) return `an integrator: ${rate}, ${delay}`;
	return `gain ${m.k.toFixed(2)} °C per %, lag ${m.t1.toFixed(0)} s${m.t2 > 0 ? ` and ${m.t2.toFixed(1)} s` : ''}, ${delay} (${rate})`;
}
