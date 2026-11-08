/**
 * The Phases tab's numbers (Roast page and History): phase lengths, their share of the
 * roast and DTR. Times are counted from CHARGE, as the server counts the roast time, so
 * the seconds between Start and CHARGE are not part of any phase.
 */
export interface PhaseEvent {
	event_type: string;
	elapsed_seconds: number;
	temperature?: number | null;
}

export interface PhaseTimes {
	/** The roast so far or in all, s from CHARGE: DROP, else the stored total, else now. */
	total: number | null;
	drying: number | null;
	maillard: number | null;
	development: number | null;
	/** Development's share of the roast, % (to now while roasting). */
	dtr: number | null;
}

/**
 * `totalTimeSeconds` is the server's roast time (CHARGE → DROP); `sinceCharge` the time
 * since CHARGE now, for a roast still going.
 */
export function phaseTimes(
	events: PhaseEvent[],
	totalTimeSeconds: number | null,
	sinceCharge: number | null = null
): PhaseTimes {
	const at = (t: string) => events.find((e) => e.event_type === t)?.elapsed_seconds;
	const charge = at('charge') ?? 0;
	const dry = at('drying_end');
	const fc = at('first_crack_start');
	const drop = at('drop');
	const total = drop != null ? drop - charge : (totalTimeSeconds ?? sinceCharge);
	const end = drop ?? (total != null ? charge + total : null);
	const development = fc != null && end != null && end > fc ? end - fc : null;
	return {
		total: total != null && total > 0 ? total : null,
		drying: dry != null ? dry - charge : null,
		maillard: dry != null && fc != null ? fc - dry : null,
		development: fc != null && drop != null ? drop - fc : null,
		dtr: development != null && total ? Math.round((development / total) * 1000) / 10 : null
	};
}
