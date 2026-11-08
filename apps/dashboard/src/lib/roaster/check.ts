/**
 * A roaster against the interface standard (docs/add-roaster.md, "roast roaster check"): the
 * read-only check's lines in the order the page shows them, their words and colour tokens,
 * the reading, and the owner's write check. The server decides every verdict; this only shows it.
 */
import type { CheckLine, CheckStatus, WriteCheck } from '#lib/api/adding.js';

const CHECK_LABEL: Record<string, string> = {
	rate: 'Rate',
	bt_et: 'BT and ET',
	units: 'Units',
	outputs: 'Outputs',
	channels: 'Channels',
	caps: 'Can do',
	control: 'Control'
};

const ORDER = ['rate', 'bt_et', 'units', 'outputs', 'channels', 'caps', 'control'];

export function checkLabel(check: string): string {
	return CHECK_LABEL[check] ?? check.replace(/_/g, ' ');
}

/** The lines in the standard's order; a check the dashboard does not know goes last. */
export function orderedLines(lines: CheckLine[]): CheckLine[] {
	const at = (c: string) => {
		const i = ORDER.indexOf(c);
		return i < 0 ? ORDER.length : i;
	};
	return [...lines].sort((a, b) => at(a.check) - at(b.check));
}

const RANK: Record<CheckStatus, number> = { ok: 0, warn: 1, fail: 2 };

/** The worst of the lines: what the verdict is. */
export function worst(statuses: CheckStatus[]): CheckStatus {
	return statuses.reduce<CheckStatus>((w, s) => (RANK[s] > RANK[w] ? s : w), 'ok');
}

/** The colour token for a status (Chart paper: go, highlight, destructive). */
export function statusClass(s: CheckStatus): string {
	return s === 'ok' ? 'text-go' : s === 'warn' ? 'text-highlight' : 'text-destructive';
}

export function verdictText(s: CheckStatus): string {
	return s === 'ok'
		? 'Meets the standard'
		: s === 'warn'
			? 'Works, with something to look at'
			: 'Does not meet the standard yet';
}

/** How a reading's key is shown, and whether it is a temperature (°C, shown in the owner's unit). */
const READING: Record<string, { label: string; temp: boolean }> = {
	bt: { label: 'BT', temp: true },
	et: { label: 'ET', temp: true },
	ambient: { label: 'Ambient', temp: true },
	setpoint: { label: 'Setpoint', temp: true },
	heater: { label: 'Heater', temp: false },
	fan: { label: 'Fan', temp: false },
	drum: { label: 'Drum', temp: false },
	damper: { label: 'Damper', temp: false }
};

export interface ReadingItem {
	key: string;
	label: string;
	value: number;
	/** °C: show in the owner's unit; else % for the outputs, or as it is for an extra. */
	kind: 'temp' | 'pct' | 'other';
}

/** The reading in a fixed order: temperatures, outputs, then the extras by name. */
export function readingItems(reading: Record<string, number> | null | undefined): ReadingItem[] {
	if (!reading) return [];
	const known = Object.keys(READING);
	const items: ReadingItem[] = [];
	for (const key of known) {
		const v = reading[key];
		if (typeof v === 'number' && Number.isFinite(v))
			items.push({
				key,
				label: READING[key].label,
				value: v,
				kind: READING[key].temp ? 'temp' : 'pct'
			});
	}
	for (const key of Object.keys(reading).sort()) {
		const v = reading[key];
		if (known.includes(key) || typeof v !== 'number' || !Number.isFinite(v)) continue;
		items.push({ key, label: key, value: v, kind: 'other' });
	}
	return items;
}

/** What the owner is asked before the write check runs: exactly what it sends. */
export const WRITE_CHECK_CONFIRM =
	'The write check sends heater 10 % for up to 5 s, then emergency stop (always, also on a timeout or an error). ' +
	'The roaster must be cold (BT below 50 °C), with no roast open and its PID in manual. Run it now?';

/** The write check's result in a line. */
export function writeSummary(r: WriteCheck): string {
	const failed = r.steps.find((s) => !s.ok);
	if (r.ok)
		return `Passed in ${(Math.max(0, ...r.steps.map((s) => s.at_ms)) / 1000).toFixed(1)} s: the roaster obeys and stops.`;
	return failed ? `Failed at “${failed.step}”: ${failed.message}` : 'Failed.';
}
