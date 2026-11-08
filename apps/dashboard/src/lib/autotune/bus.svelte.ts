/** Counters the tuning views watch to refetch after one of them changes something. */
export const tuningBus = $state({ runs: 0, schedule: 0, pid: 0 });

export function bump(...what: ('runs' | 'schedule' | 'pid')[]) {
	for (const w of what) tuningBus[w] += 1;
}
