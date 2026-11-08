/**
 * The cockpit's numbers: time since CHARGE, profile BT and RoR now, Δ to the
 * profile and development. Same time base as the scope (profile t = 0 is
 * CHARGE, shifted by a running replay's alignment; docs/replay.md).
 */
import {
	profileCurve,
	recordedCurve,
	alignToBase,
	profileRor,
	profileDtr,
	type Num
} from '#lib/scope/scopeData.js';
import type { ProfileWithPoints } from '#lib/api/profiles.js';
import { elapsedSinceStart, roastStage, nextLandmark, type EventLike } from './live.js';

export interface ProfileTrack {
	t: number[];
	bt: Num[];
	ror: Num[];
	/** Development target from the profile's FC and total time, %. */
	targetDtr: number | null;
}

/** The profile on the scope's time base (call when the profile or shift changes, not every tick). */
export function profileTrack(p: ProfileWithPoints | null, shiftSec = 0): ProfileTrack | null {
	if (!p) return null;
	const c = p.curves?.bt?.length
		? recordedCurve(p.curves, shiftSec)
		: profileCurve(p.points, shiftSec);
	if (c.t.length === 0) return null;
	return {
		t: c.t,
		bt: c.bt,
		ror: profileRor(c.t, c.bt),
		targetDtr: profileDtr(p.target_first_crack, p.target_total_time)
	};
}

export interface RoastNumbers {
	/** Seconds since the session started. */
	sinceStart: number;
	/** Seconds since CHARGE, or null before CHARGE. */
	sinceCharge: number | null;
	/** Profile BT and RoR now (null without a profile or outside it). */
	profBt: number | null;
	profRor: number | null;
	/** BT − profile BT, °C (positive: ahead/hotter). */
	delta: number | null;
	/** Development: seconds since first crack and the DTR so far, %. */
	devSec: number | null;
	dtr: number | null;
}

export function roastNumbers(o: {
	nowMs: number;
	session: { start_time: string | null } | null;
	events: EventLike[];
	track: ProfileTrack | null;
	bt: number | null | undefined;
}): RoastNumbers {
	const sinceStart = elapsedSinceStart(o.session, o.nowMs);
	const charge = o.events.find((e) => e.event_type === 'charge');
	const sinceCharge = o.session?.start_time && charge ? sinceStart - charge.elapsed_seconds : null;
	// Scope time: since CHARGE, or since the start before it.
	const t = sinceCharge ?? sinceStart;
	let profBt: number | null = null;
	let profRor: number | null = null;
	if (o.session?.start_time && o.track) {
		profBt = alignToBase([t], o.track.t, o.track.bt)[0];
		profRor = alignToBase([t], o.track.t, o.track.ror)[0];
	}
	const delta = profBt != null && o.bt != null && Number.isFinite(o.bt) ? o.bt - profBt : null;
	const fc = o.events.find((e) => e.event_type === 'first_crack_start');
	let devSec: number | null = null;
	let dtr: number | null = null;
	if (fc && charge && sinceCharge != null && sinceCharge > 0) {
		const drop = o.events.find((e) => e.event_type === 'drop');
		const end = drop ? drop.elapsed_seconds - charge.elapsed_seconds : sinceCharge;
		devSec = Math.max(0, end - (fc.elapsed_seconds - charge.elapsed_seconds));
		dtr = end > 0 ? (devSec / end) * 100 : null;
	}
	return { sinceStart, sinceCharge, profBt, profRor, delta, devSec, dtr };
}

/** How far off the profile is still fine (°C): green ≤ 2, amber ≤ 5, red beyond. */
export function deltaLevel(delta: number | null): 'ok' | 'warn' | 'bad' | null {
	if (delta == null) return null;
	const a = Math.abs(delta);
	return a <= 2 ? 'ok' : a <= 5 ? 'warn' : 'bad';
}

export type PrimaryAction =
	| { kind: 'start' }
	| { kind: 'resume' }
	| { kind: 'mark'; landmark: string }
	| { kind: 'complete' };

/** The one button each stage leads with. */
export function primaryAction(
	session: { status: string } | null,
	events: EventLike[]
): PrimaryAction {
	const stage = roastStage(session, events);
	if (stage === 'setup') return { kind: 'start' };
	if (session?.status === 'paused') return { kind: 'resume' };
	if (stage === 'charge') return { kind: 'mark', landmark: 'charge' };
	if (stage === 'roasting') return { kind: 'mark', landmark: nextLandmark(events) };
	return { kind: 'complete' };
}

/** Button words for each landmark. */
export const LANDMARK_ACTION: Record<string, string> = {
	charge: 'Charge',
	turning_point: 'Turning point',
	drying_end: 'Dry end',
	first_crack_start: 'First crack',
	first_crack_end: 'First crack end',
	second_crack_start: 'Second crack',
	second_crack_end: 'Second crack end',
	drop: 'Drop',
	cool_end: 'Cool end'
};
