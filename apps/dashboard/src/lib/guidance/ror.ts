/**
 * RoR events around first crack (docs/roast-guidance.md, `ror.ts`). These are Roastwarden's own
 * heuristics: no source gives numeric thresholds, so they name the signal, not a defect
 * ("RoR rose after first crack", not "baked"), and feed the summary only. Tune the constants
 * on real roasts.
 *
 * Input is the 30 s-smoothed RoR (`rorSeries(telemetry, chargeAt)`, slope over ±15 s) as
 * [roast time s, °C/min], sorted.
 */

/** A rise after FC this far above the lowest RoR since FC, °C/min… */
const RISE_MIN = 2;
/** …held at least this long, s. */
const RISE_HOLD_S = 20;
/** A fall this large, °C/min… */
const CRASH_MIN = 8;
/** …within this window centred on FC, s. */
const CRASH_WINDOW_S = 60;
/** RoR at or below this, °C/min… */
const STALL_MAX = 0.5;
/** …for at least this long after FC, s. */
const STALL_HOLD_S = 30;

type RorEventKind = 'rise' | 'crash' | 'stall';

export interface RorEvent {
	kind: RorEventKind;
	/** Roast time the event starts, s. */
	at: number;
	/** rise / crash: °C/min; stall: seconds. */
	size: number;
}

export function rorEvents(
	ror: [number, number][],
	fc: number | null | undefined,
	drop: number | null | undefined
): RorEvent[] {
	if (fc == null || !ror.length) return [];
	const end = drop ?? ror[ror.length - 1][0];
	const out: RorEvent[] = [];

	// Crash: the largest peak-to-later-trough fall inside FC ± 30 s.
	const half = CRASH_WINDOW_S / 2;
	let peak = -Infinity;
	let peakAt = fc;
	let fall = 0;
	for (const [t, r] of ror) {
		if (t < fc - half || t > fc + half) continue;
		if (r > peak) {
			peak = r;
			peakAt = t;
		}
		fall = Math.max(fall, peak - r);
	}
	if (fall >= CRASH_MIN) out.push({ kind: 'crash', at: peakAt, size: fall });

	// Rise: RoR above the lowest value since FC by RISE_MIN, held RISE_HOLD_S.
	let low = Infinity;
	let riseFrom: number | null = null;
	let riseMax = 0;
	for (const [t, r] of ror) {
		if (t < fc || t > end) continue;
		if (riseFrom == null) low = Math.min(low, r);
		if (r - low >= RISE_MIN) {
			riseFrom ??= t;
			riseMax = Math.max(riseMax, r - low);
			if (t - riseFrom >= RISE_HOLD_S) {
				out.push({ kind: 'rise', at: riseFrom, size: riseMax });
				break;
			}
		} else {
			riseFrom = null;
			riseMax = 0;
			low = Math.min(low, r);
		}
	}

	// Stall: RoR at or below STALL_MAX for STALL_HOLD_S after FC (the first such run).
	let from: number | null = null;
	let last = fc;
	for (const [t, r] of ror) {
		if (t < fc || t > end) continue;
		if (r <= STALL_MAX) {
			from ??= t;
			last = t;
			continue;
		}
		if (from != null && last - from >= STALL_HOLD_S) break;
		from = null;
	}
	if (from != null && last - from >= STALL_HOLD_S)
		out.push({ kind: 'stall', at: from, size: last - from });
	return out.sort((a, b) => a.at - b.at);
}

export function rorEventText(
	e: RorEvent,
	fmtRate: (cPerMin: number) => string = (v) => `${Math.round(v)} °C/min`
): string {
	switch (e.kind) {
		case 'rise':
			return `RoR rose after first crack (up ${fmtRate(e.size)}).`;
		case 'crash':
			return `RoR dropped sharply at first crack (down ${fmtRate(e.size)}).`;
		case 'stall':
			return `BT stalled after first crack (flat for ${Math.round(e.size)} s).`;
	}
}
