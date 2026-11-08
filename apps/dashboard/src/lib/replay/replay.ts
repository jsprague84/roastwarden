/**
 * Pure helpers for the replay panel: what a profile can drive, how channel
 * values, event ETAs and log entries read. Temperatures arrive in °C; callers
 * pass a formatter for the display unit.
 */
import type { ProfileWithPoints, ProfileEventInput, RoastProfile } from '#lib/api/profiles.js';
import type {
	ReplayChannel,
	ReplayChannelState,
	ReplayEventView,
	ReplayLogEntry,
	ReplayMode,
	LoopPref,
	AlignOn,
	ReplayTrigger,
	ReplayState
} from '#lib/api/replay.js';
import { describeEvent } from '#lib/profile/events.js';
import { fmtClock } from '#lib/scope/scopeData.js';

export type TempFmt = (c: number) => string;

export const MODE_LABELS: Record<ReplayMode, string> = {
	setpoint: 'Setpoint (PID on BT)',
	feedforward: 'Feedforward (heater %)',
	feedforward_pid: 'Feedforward + PID'
};

export const LOOP_LABELS: Record<LoopPref, string> = {
	auto: 'Auto',
	device: 'On the roaster',
	server: 'On the server'
};

export const TRIGGER_LABELS: Record<ReplayTrigger, string> = {
	profile: "Each event's own",
	time: 'By time',
	bt: 'By BT'
};

export const ALIGN_LABELS: Record<AlignOn, string> = {
	charge: 'CHARGE',
	turning_point: 'TP',
	drying_end: 'DRY',
	first_crack_start: 'FC'
};

export const CHANNEL_LABELS: Record<ReplayChannel, string> = {
	setpoint: 'Setpoint',
	heater: 'Heater',
	fan: 'Fan'
};

/** Whether the profile has heater data (points or events): feedforward needs it. */
export function profileHasHeater(p: ProfileWithPoints | null): boolean {
	if (!p) return false;
	return (
		p.points.some((x) => x.heater_pct != null) ||
		(p.events ?? []).some((e) => e.action === 'heater')
	);
}

/** Whether the profile has fan data (points or events). */
export function profileHasFan(p: ProfileWithPoints | null): boolean {
	if (!p) return false;
	return (
		p.points.some((x) => x.fan_speed != null) || (p.events ?? []).some((e) => e.action === 'fan')
	);
}

/** "185°" for setpoint, "62%" for heater and fan, "--" when unknown. */
export function fmtChannelValue(
	ch: ReplayChannel,
	v: number | null | undefined,
	temp: TempFmt
): string {
	if (v == null || !Number.isFinite(v)) return '--';
	return ch === 'setpoint' ? temp(v) : `${Math.round(v)}%`;
}

/** Driven channels first, in a stable order. */
export function sortChannels(chs: ReplayChannelState[]): ReplayChannelState[] {
	const order: ReplayChannel[] = ['setpoint', 'heater', 'fan'];
	return chs
		.slice()
		.sort(
			(a, b) =>
				Number(b.driven) - Number(a.driven) || order.indexOf(a.channel) - order.indexOf(b.channel)
		);
}

/** Countdown text for an upcoming event: "0:42", "~1:10" for BT estimates, "waiting" for delays. */
export function fmtEta(e: ReplayEventView): string {
	if (e.eta_seconds == null) return '?';
	const approx = e.trigger === 'bt' && e.state === 'pending';
	return `${approx ? '~' : ''}${fmtClock(e.eta_seconds)}`;
}

/** What the trigger is, e.g. "at 2:30" or "BT 160°, 12° to go". */
export function fmtTrigger(e: ReplayEventView, temp: TempFmt, delta: TempFmt): string {
	let s: string;
	if (e.trigger === 'time') s = e.at_seconds != null ? `at ${fmtClock(e.at_seconds)}` : 'by time';
	else {
		s = e.at_bt != null ? `BT ${temp(e.at_bt)}` : 'by BT';
		if (e.to_go_c != null) s += `, ${delta(e.to_go_c)} to go`;
	}
	if (e.delay_seconds > 0) s += ` +${Math.round(e.delay_seconds)} s`;
	return s;
}

export function describeReplayEvent(
	e: Pick<ReplayEventView, 'action' | 'value' | 'label'>,
	temp: TempFmt
): string {
	return describeEvent(
		{
			trigger_kind: 'time',
			at_seconds: 0,
			at_bt: null,
			action: e.action,
			value: e.value ?? null,
			label: e.label ?? null
		} as ProfileEventInput,
		temp
	);
}

/** One line for the log tail. */
export function fmtLogEntry(l: ReplayLogEntry, temp: TempFmt): string {
	const at = fmtClock(l.live_t);
	const what = l.action
		? describeReplayEvent(
				{ action: l.action, value: l.value ?? null, label: l.label ?? null },
				temp
			)
		: '';
	const ch = l.channel ? CHANNEL_LABELS[l.channel] : '';
	switch (l.kind) {
		case 'fired':
			return `${at} ${what}`;
		case 'cue':
			return `${at} ${what} (cue)`;
		case 'held':
			return `${at} ${what} held (${ch} paused)`;
		case 'superseded':
			return `${at} ${what} superseded`;
		case 'skipped':
			return `${at} ${what} skipped${l.detail ? `: ${l.detail}` : ''}`;
		case 'delayed':
			return `${at} ${what} ${l.detail ?? 'delayed'}`;
		case 'paused':
			return `${at} ${ch} paused${l.detail ? ` (${l.detail})` : ''}`;
		case 'resumed':
			return `${at} ${ch} resumed`;
		default:
			return `${at} ${l.detail ?? l.kind}`;
	}
}

/**
 * Profile time (s) at `nowMs`: seconds since CHARGE (the session start when
 * no CHARGE is marked) minus the replay's align offset. Profile t = 0 is CHARGE.
 */
export function profileTimeSec(
	nowMs: number,
	sessionStartMs: number,
	landmarks: { type: string; elapsed_seconds: number }[],
	alignOffsetSec = 0
): number {
	const charge = landmarks.find((l) => l.type === 'charge')?.elapsed_seconds ?? 0;
	return (nowMs - sessionStartMs) / 1000 - charge - alignOffsetSec;
}

/**
 * The Profile drive tab's label: its name when idle; while this session's drive runs, a
 * short name and the state with the profile time ("Live 4:12", "Ghost 4:12", "Paused 4:12"
 * when you have taken a channel back), so the state shows from any tab.
 */
export function driveTabLabel(
	st:
		| Pick<ReplayState, 'active' | 'session_id' | 'profile_t' | 'config' | 'channels'>
		| null
		| undefined,
	sessionId: string | null | undefined
): { label: string; status: string | null; tone: 'live' | 'ghost' | 'paused' | null } {
	if (!st?.active || !sessionId || st.session_id !== sessionId)
		return { label: 'Profile drive', status: null, tone: null };
	const paused = !st.config.ghost && st.channels.some((c) => c.driven && c.paused);
	const tone = st.config.ghost ? 'ghost' : paused ? 'paused' : 'live';
	const word = tone === 'ghost' ? 'Ghost' : tone === 'paused' ? 'Paused' : 'Live';
	return { label: 'Drive', status: `${word} ${fmtClock(Math.max(0, st.profile_t))}`, tone };
}

/**
 * The "drive stopped" banner for the Roast page, or null: the roaster cut its heater, or a
 * server restart cut the drive (`restart`, with the profile time a resume picks up at). Shown
 * while the roast is on and not dropped, until a new run starts.
 */
export function cutBanner(
	st:
		| Pick<
				ReplayState,
				| 'active'
				| 'session_id'
				| 'stopped_by_device'
				| 'stopped_by_restart'
				| 'stopped_reason'
				| 'live_t'
				| 'profile_t'
		  >
		| null
		| undefined,
	sessionId: string | null | undefined,
	dropped: boolean
): { at: string; why: string; restart: boolean; profileAt: string } | null {
	if (
		!st ||
		st.active ||
		!(st.stopped_by_device || st.stopped_by_restart) ||
		!sessionId ||
		st.session_id !== sessionId ||
		dropped
	)
		return null;
	const at = fmtClock(Math.max(0, st.live_t ?? 0));
	const profileAt = fmtClock(Math.max(0, st.profile_t ?? 0));
	if (st.stopped_by_restart) return { at, why: 'the server restarted', restart: true, profileAt };
	const named = /^the roaster cut its heater: (.+)$/.exec(st.stopped_reason ?? '');
	return {
		at,
		why: named ? named[1] : 'safe state, emergency stop or an interlock',
		restart: false,
		profileAt
	};
}

/**
 * The Roast page's auto DROP line (docs/replay.md, "Auto DROP"), or null when there is none to
 * show: no auto DROP, another roast, a stopped or ghost drive, DROP already marked.
 * `counting` asks for the loud form (the countdown, Cancel first).
 */
export function autoDropLine(
	st: Pick<ReplayState, 'active' | 'session_id' | 'auto_drop' | 'config'> | null | undefined,
	sessionId: string | null | undefined,
	dropped: boolean
): { state: 'armed' | 'counting' | 'due' | 'cancelled'; text: string; title: string } | null {
	const a = st?.auto_drop;
	if (
		!st ||
		!a ||
		!st.active ||
		!sessionId ||
		st.session_id !== sessionId ||
		dropped ||
		a.ghost ||
		st.config?.ghost
	)
		return null;
	const what = a.describe;
	const title = `Profile drive marks DROP by itself ${what}, after a ${Math.round(a.countdown_seconds)} s countdown, so the roast sequence runs DROP's step (heater off, fan to cool). Cancel for this roast to roast on by hand.`;
	switch (a.state) {
		case 'counting':
			return {
				state: 'counting',
				text: `Auto DROP in ${Math.max(0, Math.ceil(a.eta_seconds ?? 0))} s`,
				title
			};
		case 'due':
			return { state: 'due', text: 'Auto DROP: dropping…', title };
		case 'cancelled':
			return { state: 'cancelled', text: 'Auto DROP cancelled for this roast', title };
		default: {
			const when = a.waiting_for
				? ` · after ${a.waiting_for}`
				: a.eta_seconds != null && a.eta_seconds > 0
					? ` · ${fmtClock(a.eta_seconds)} to go`
					: '';
			return { state: 'armed', text: `Auto DROP ${what}${when}`, title };
		}
	}
}

/** A profile's auto DROP in a few words ("auto DROP at 10:05"), or null when it is off. */
export function autoDropFact(
	p: Pick<
		RoastProfile,
		'auto_drop' | 'target_total_time' | 'target_first_crack' | 'target_end_temp'
	>,
	temp: TempFmt
): string | null {
	const total = p.target_total_time;
	const fc = p.target_first_crack;
	switch (p.auto_drop) {
		case 'time':
			return total != null ? `auto DROP at ${fmtClock(total)}` : 'auto DROP at the drop time';
		case 'bt':
			return p.target_end_temp != null
				? `auto DROP at ${temp(p.target_end_temp)}`
				: 'auto DROP at the end temperature';
		case 'development':
			return total != null && fc != null
				? `auto DROP ${fmtClock(total - fc)} after first crack`
				: 'auto DROP after first crack';
		default:
			return null;
	}
}

/** A resumed run's tooltip for its "(+0:40)" after the profile time, or null. */
export function holdNote(st: Pick<ReplayState, 'config'> | null | undefined): string | null {
	const h = st?.config?.hold?.seconds;
	return h && h > 0
		? `Resumed after a restart: the profile runs ${fmtClock(h)} later for the time lost`
		: null;
}
