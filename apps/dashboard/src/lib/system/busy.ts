import type { Busy, BusyReason } from '#lib/api/about.js';

const WORDS: Record<BusyReason, string> = {
	drive: 'profile drive',
	autotune: 'autotune',
	cooling: 'cooling after DROP',
	heating: 'heater on',
	roasting: 'roast with hot beans'
};

/** One line per busy roaster for Settings → About; empty when nothing is busy. */
export function busyLines(b: Busy | null | undefined): string[] {
	return (b?.roasters ?? []).map(
		(r) =>
			`${r.device_id}${r.virtual ? ' (virtual, does not hold deploys)' : ''}: ${r.reasons.map((x) => WORDS[x] ?? x).join(', ')}`
	);
}
