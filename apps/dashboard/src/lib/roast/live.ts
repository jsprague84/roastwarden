/**
 * Pure rules for the roast in progress: which session is "the" roast on a
 * roaster, which profile the scope shows, what stage the roast is in and
 * what the next step is. The roast store (stores/roast.svelte.ts) applies
 * them; keeping them here makes them testable without a browser.
 */
import type { RoastSession } from '#lib/api/sessions.js';
import type { VirtualRoasterState } from '#lib/api/virtual.js';

/** The server's simulated roaster (docs/virtual-roaster.md). */
export const VIRTUAL_DEVICE_ID = 'virtual_roaster';
/** The simulated drum roaster beside it: a stranger's roaster on purpose (docs/add-roaster.md). */
export const VIRTUAL_DRUM_ID = 'virtual_drum';

/** A demo visitor's own simulated fluid bed (`virtual_` and 8 hex digits; docs/DEPLOY.md). */
export function isVisitorRoaster(deviceId: string | null | undefined): boolean {
	return !!deviceId && /^virtual_[0-9a-f]{8}$/.test(deviceId);
}

/** A simulated fluid bed: the shared `virtual_roaster` or a demo visitor's own. */
export function isSimulatedFluidBed(deviceId: string | null | undefined): boolean {
	return deviceId === VIRTUAL_DEVICE_ID || isVisitorRoaster(deviceId);
}

/** One of the server's simulators: the fluid bed (shared or a demo visitor's) or the drum. */
export function isVirtual(deviceId: string | null | undefined): boolean {
	return isSimulatedFluidBed(deviceId) || deviceId === VIRTUAL_DRUM_ID;
}

/** The simulated drum (it is set up like any roaster; only its beans are the simulator's). */
export function isVirtualDrum(deviceId: string | null | undefined): boolean {
	return deviceId === VIRTUAL_DRUM_ID;
}

/** Whether the simulator behind `deviceId` has beans in: the drum's, or the fluid bed's. */
export function simLoaded(
	vr: Pick<VirtualRoasterState, 'loaded' | 'drum'>,
	deviceId: string | null | undefined
): boolean {
	return isVirtualDrum(deviceId) ? !!vr.drum?.loaded : vr.loaded;
}

/** A session that is recording (active) or paused. */
function isLiveSession(s: Pick<RoastSession, 'status'> | null | undefined): boolean {
	return !!s && (s.status === 'active' || s.status === 'paused');
}

/**
 * The roast running on `deviceId`: its newest active or paused session.
 * Sessions on other roasters never count, so two roasters can roast at once
 * without taking over each other's scope.
 */
export function pickActiveSession(
	list: RoastSession[],
	deviceId: string | null
): RoastSession | null {
	if (!deviceId) return null;
	const live = list.filter((s) => s.device_id === deviceId && isLiveSession(s));
	if (live.length === 0) return null;
	const key = (s: RoastSession) => new Date(s.start_time ?? s.created_at).getTime() || 0;
	return live.reduce((a, b) => (key(b) > key(a) ? b : a));
}

/**
 * The profile the scope overlays: the running session's own profile (the
 * single source of truth once a roast started), else the one picked for the
 * next roast.
 */
export function wantedProfileId(
	session: Pick<RoastSession, 'profile_id'> | null,
	previewId: string | null
): string | null {
	if (session) return session.profile_id ?? null;
	return previewId || null;
}

export interface EventLike {
	event_type: string;
	elapsed_seconds: number;
	temperature?: number | null;
}

/**
 * Where the roast is:
 * - `setup`: no session yet (pick roaster and profile, then start);
 * - `charge`: recording, waiting for the beans to go in;
 * - `roasting`: charged, not dropped;
 * - `dropped`: DROP marked, cooling and wrapping up.
 */
export type RoastStage = 'setup' | 'charge' | 'roasting' | 'dropped';

export function roastStage(
	session: Pick<RoastSession, 'status'> | null,
	events: EventLike[]
): RoastStage {
	if (!isLiveSession(session)) return 'setup';
	const has = (t: string) => events.some((e) => e.event_type === t);
	if (has('drop')) return 'dropped';
	if (has('charge')) return 'roasting';
	return 'charge';
}

/** Landmarks in roast order (keyboard 1-8 follows this order). */
export const LANDMARK_ORDER = [
	'charge',
	'drying_end',
	'first_crack_start',
	'first_crack_end',
	'second_crack_start',
	'second_crack_end',
	'drop',
	'cool_end'
] as const;

/** What usually comes next while roasting; second crack is optional, so DROP follows FC end. */
const ROASTING_PATH = ['drying_end', 'first_crack_start', 'first_crack_end', 'drop'] as const;

/**
 * The landmark to offer as the primary action while roasting: the step
 * after the furthest one marked on the usual path (a missed DRY does not
 * hold the roast back once FC is in).
 */
export function nextLandmark(events: EventLike[]): string {
	const marked = new Set(events.map((e) => e.event_type));
	let furthest = -1;
	ROASTING_PATH.forEach((t, i) => {
		if (marked.has(t)) furthest = i;
	});
	for (let i = furthest + 1; i < ROASTING_PATH.length; i++) {
		if (!marked.has(ROASTING_PATH[i])) return ROASTING_PATH[i];
	}
	return 'drop';
}

/** Seconds since the session started (what the server stores for events). */
export function elapsedSinceStart(
	session: Pick<RoastSession, 'start_time'> | null,
	nowMs: number
): number {
	if (!session?.start_time) return 0;
	return Math.max(0, (nowMs - new Date(session.start_time).getTime()) / 1000);
}

/** Seconds since CHARGE, or null before it is marked. */
export function sinceCharge(
	session: Pick<RoastSession, 'start_time'> | null,
	events: EventLike[],
	nowMs: number
): number | null {
	const charge = events.find((e) => e.event_type === 'charge');
	if (!session?.start_time || !charge) return null;
	return elapsedSinceStart(session, nowMs) - charge.elapsed_seconds;
}

/** BT within this of the first-crack target: the Roast page says first crack is due. */
const FC_DUE_MARGIN_C = 2;

/**
 * First crack is due: BT is still rising and within `FC_DUE_MARGIN_C` of the profile's
 * first-crack BT (else a typical 196 °C). Under closed-loop control the RoR dip that a
 * crack causes is hidden by the PID (it came 45 s to 2½ min late on the real roasts), so
 * the prompt goes by temperature and the ear decides.
 */
export function fcDue(
	bt: number | null | undefined,
	ror: number | null | undefined,
	target: number | null | undefined
): boolean {
	if (bt == null || !Number.isFinite(bt)) return false;
	if (ror == null || !(ror > 0)) return false;
	return bt >= (target ?? 196) - FC_DUE_MARGIN_C;
}
