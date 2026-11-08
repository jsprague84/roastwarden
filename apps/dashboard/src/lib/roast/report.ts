/** Roast report numbers: all times from CHARGE, temperatures °C. */
import type { RoastEvent } from '#lib/api/events.js';
import type { SessionTelemetryPoint } from '#lib/api/sessions.js';

export interface Mark {
	t: number;
	temp: number | null;
}

export interface Summary {
	/** Session elapsed seconds at CHARGE. */
	chargeAt: number;
	tp: Mark | null;
	dry: Mark | null;
	fc: Mark | null;
	drop: Mark | null;
	/** CHARGE → DROP (else to the last sample), s. */
	total: number | null;
	/** % */
	dtr: number | null;
}

function mark(events: RoastEvent[], type: string, chargeAt: number): Mark | null {
	const e = events.find((x) => x.event_type === type);
	return e ? { t: e.elapsed_seconds - chargeAt, temp: e.temperature } : null;
}

export function roastSummary(events: RoastEvent[], telemetry: SessionTelemetryPoint[]): Summary {
	const chargeAt = events.find((e) => e.event_type === 'charge')?.elapsed_seconds ?? 0;
	const drop = mark(events, 'drop', chargeAt);
	const fc = mark(events, 'first_crack_start', chargeAt);
	const last = telemetry.length ? telemetry[telemetry.length - 1].elapsed_seconds - chargeAt : null;
	const total = drop?.t ?? last;
	const dtr = fc && total && total > fc.t ? ((total - fc.t) / total) * 100 : null;
	return {
		chargeAt,
		tp: mark(events, 'turning_point', chargeAt),
		dry: mark(events, 'drying_end', chargeAt),
		fc,
		drop,
		total,
		dtr
	};
}

/** BT at roast time `t` (s from CHARGE), linear between samples. */
export function btAt(
	telemetry: SessionTelemetryPoint[],
	chargeAt: number,
	t: number
): number | null {
	let prev: SessionTelemetryPoint | null = null;
	for (const p of telemetry) {
		if (p.bean_temp == null) continue;
		const pt = p.elapsed_seconds - chargeAt;
		if (pt >= t) {
			if (!prev || prev.bean_temp == null) return p.bean_temp;
			const qt = prev.elapsed_seconds - chargeAt;
			const f = pt === qt ? 0 : (t - qt) / (pt - qt);
			return prev.bean_temp + (p.bean_temp - prev.bean_temp) * f;
		}
		prev = p;
	}
	return prev?.bean_temp ?? null;
}

export interface PhaseRow {
	key: 'drying' | 'maillard' | 'development';
	label: string;
	duration: number;
	/** % of CHARGE → DROP */
	pct: number | null;
	/** BT rise over the phase, °C */
	dT: number | null;
	/** °C/min */
	ror: number | null;
}

/** Drying CHARGE→DRY, Maillard DRY→FC, development FC→DROP; rows only where both ends exist. */
export function phaseRows(sum: Summary, telemetry: SessionTelemetryPoint[]): PhaseRow[] {
	const end = sum.drop?.t ?? sum.total;
	const spans: [PhaseRow['key'], string, number | null | undefined, number | null | undefined][] = [
		['drying', 'Drying', 0, sum.dry?.t],
		['maillard', 'Maillard', sum.dry?.t, sum.fc?.t],
		['development', 'Development', sum.fc?.t, end]
	];
	const rows: PhaseRow[] = [];
	for (const [key, label, a, b] of spans) {
		if (a == null || b == null || b <= a) continue;
		// Drying ΔT and RoR run from the turning point, as Artisan's do: from
		// CHARGE they include the drum's dip and come out negative.
		const from = key === 'drying' && sum.tp && sum.tp.t > a && sum.tp.t < b ? sum.tp.t : a;
		const ta = btAt(telemetry, sum.chargeAt, from);
		const tb = btAt(telemetry, sum.chargeAt, b);
		const dT = ta != null && tb != null ? tb - ta : null;
		rows.push({
			key,
			label,
			duration: b - a,
			pct: sum.total ? ((b - a) / sum.total) * 100 : null,
			dT,
			ror: dT != null && b > from ? (dT / (b - from)) * 60 : null
		});
	}
	return rows;
}

/** BT rate of rise (°C/min) at each sample: slope over ±`half` seconds. Times are roast time. */
export function rorSeries(
	telemetry: SessionTelemetryPoint[],
	chargeAt: number,
	half = 15
): [number, number][] {
	const pts = telemetry
		.filter((p) => p.bean_temp != null)
		.map((p) => [p.elapsed_seconds - chargeAt, p.bean_temp as number] as [number, number]);
	const out: [number, number][] = [];
	let lo = 0;
	let hi = 0;
	for (let i = 0; i < pts.length; i++) {
		const t = pts[i][0];
		while (lo < i && pts[lo][0] < t - half) lo++;
		if (hi < i) hi = i;
		while (hi + 1 < pts.length && pts[hi + 1][0] <= t + half) hi++;
		const span = pts[hi][0] - pts[lo][0];
		if (span >= 5) out.push([t, ((pts[hi][1] - pts[lo][1]) / span) * 60]);
	}
	return out;
}

export function fmtMmSs(s: number | null | undefined): string {
	if (s == null || !Number.isFinite(s)) return '–';
	const sign = s < 0 ? '-' : '';
	const a = Math.round(Math.abs(s));
	return `${sign}${Math.floor(a / 60)}:${String(a % 60).padStart(2, '0')}`;
}
