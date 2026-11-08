/** Ranking table over sessions (History): columns, sort, filter, CSV. */
import type { RoastSession } from '#lib/api/sessions.js';
import { purposeLabel } from './properties.js';

export type ColKey =
	| 'batch'
	| 'date'
	| 'name'
	| 'bean'
	| 'purpose'
	| 'duration'
	| 'fc'
	| 'dtr'
	| 'maxBt'
	| 'loss'
	| 'colorWhole'
	| 'colorGround'
	| 'rmse'
	| 'rating';

export interface Column {
	key: ColKey;
	label: string;
	/** Raw value for sorting and CSV (°C, seconds, %). */
	value: (s: RoastSession) => string | number | null;
	numeric: boolean;
	title?: string;
}

const pct = (v: number | null | undefined) => (v == null ? null : Math.round(v * 1000) / 10);

export const COLUMNS: Column[] = [
	{
		key: 'batch',
		label: '#',
		value: (s) => s.batch_number ?? null,
		numeric: true,
		title: 'Batch number on its roaster'
	},
	{ key: 'date', label: 'Date', value: (s) => s.start_time ?? s.created_at, numeric: false },
	{ key: 'name', label: 'Name', value: (s) => s.name, numeric: false },
	{
		key: 'bean',
		label: 'Bean',
		value: (s) => [s.bean_origin, s.bean_variety].filter(Boolean).join(' · ') || null,
		numeric: false
	},
	{
		key: 'purpose',
		label: 'For',
		value: (s) => (s.roast_purpose ? purposeLabel(s.roast_purpose) : null),
		numeric: false
	},
	{
		key: 'duration',
		label: 'Time',
		value: (s) => s.total_time_seconds,
		numeric: true,
		title: 'CHARGE → DROP'
	},
	{
		key: 'fc',
		label: 'FC',
		value: (s) => s.first_crack_time,
		numeric: true,
		title: 'First crack, time from CHARGE'
	},
	{
		key: 'dtr',
		label: 'DTR %',
		value: (s) => pct(s.development_time_ratio),
		numeric: true,
		title: 'Development time ratio'
	},
	{ key: 'maxBt', label: 'Max BT', value: (s) => s.max_temp, numeric: true },
	{
		key: 'loss',
		label: 'Loss %',
		value: (s) => (s.weight_loss_pct == null ? null : Math.round(s.weight_loss_pct * 10) / 10),
		numeric: true
	},
	{
		key: 'colorWhole',
		label: 'Colour W',
		value: (s) => s.color_whole ?? null,
		numeric: true,
		title: 'Whole-bean colour'
	},
	{
		key: 'colorGround',
		label: 'Colour G',
		value: (s) => s.color_ground ?? null,
		numeric: true,
		title: 'Ground colour'
	},
	{
		key: 'rmse',
		label: 'RMS',
		value: (s) => (s.profile_rmse == null ? null : Math.round(s.profile_rmse * 10) / 10),
		numeric: true,
		title: 'BT vs profile, TP to DROP'
	},
	{ key: 'rating', label: 'Rating', value: (s) => s.rating ?? null, numeric: true }
];

export interface Filter {
	purpose: string;
	text: string;
	completedOnly: boolean;
}

export function filterSessions(list: RoastSession[], f: Filter): RoastSession[] {
	const q = f.text.trim().toLowerCase();
	return list.filter((s) => {
		if (f.completedOnly && s.status !== 'completed') return false;
		if (f.purpose && (s.roast_purpose ?? '') !== f.purpose) return false;
		const batch = s.batch_number != null && q.replace(/^#/, '') === String(s.batch_number);
		if (
			q &&
			!batch &&
			![s.name, s.bean_origin, s.bean_variety, s.notes].some((v) => v?.toLowerCase().includes(q))
		)
			return false;
		return true;
	});
}

/** Sort by a column; empty values always go last. */
export function sortSessions(
	list: RoastSession[],
	key: ColKey,
	dir: 'asc' | 'desc'
): RoastSession[] {
	const col = COLUMNS.find((c) => c.key === key)!;
	const sign = dir === 'asc' ? 1 : -1;
	return [...list].sort((a, b) => {
		const va = col.value(a);
		const vb = col.value(b);
		if (va == null && vb == null) return 0;
		if (va == null) return 1;
		if (vb == null) return -1;
		if (typeof va === 'number' && typeof vb === 'number') return (va - vb) * sign;
		return String(va).localeCompare(String(vb)) * sign;
	});
}

function csvCell(v: string | number | null): string {
	if (v == null) return '';
	const s = String(v);
	return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** CSV of the rows with every column (temperatures °C, times s). */
export function toCsv(list: RoastSession[]): string {
	const head = ['id', ...COLUMNS.map((c) => c.label), 'Flags', 'Notes'];
	const rows = list.map((s) =>
		[s.id, ...COLUMNS.map((c) => c.value(s)), s.roast_flags ?? null, s.notes ?? null]
			.map(csvCell)
			.join(',')
	);
	return [head.map(csvCell).join(','), ...rows].join('\n') + '\n';
}
