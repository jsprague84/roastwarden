import type { AlarmEvent } from '#lib/stores/telemetry.js';

/** An alarm heard longer ago than this is not announced: the Roast page was not open when it came. */
export const ANNOUNCE_WITHIN_MS = 15_000;

/** One alarm's identity: the same firing heard again is the same key. */
export function alarmKey(a: AlarmEvent): string {
	return `${a.session_id}:${a.name}:${a.fired_at}`;
}

/**
 * Whether the Roast page should toast and beep for this alarm. The newest alarm stays in its
 * store after it was announced, so a Roast page opened again (back from History, say) reads
 * it again: `seen` is the last key announced, kept outside the page. An alarm heard while the
 * page was closed is old news by the time it opens, and one for another roast than the one on
 * screen goes to the notification channels only.
 */
export function shouldAnnounce(
	a: AlarmEvent,
	seen: string | null,
	activeSessionId: string | null,
	now: number
): boolean {
	if (alarmKey(a) === seen) return false;
	if (a.received_at !== undefined && now - a.received_at > ANNOUNCE_WITHIN_MS) return false;
	return !activeSessionId || a.session_id === activeSessionId;
}
