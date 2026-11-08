/**
 * The server's prediction from the roaster's own model (docs/prediction.md) on the scope:
 * its BT line and 90 % band on the scope's time grid, and the readout's words.
 */
import type { ModelView, Prediction, RoastScore, RorWarning } from '#lib/api/predict.js';

export interface PredictionOnGrid {
	mid: (number | null)[];
	low: (number | null)[];
	high: (number | null)[];
}

/**
 * The prediction at each scope time `x` (s): interpolated between its points, from now (BT
 * now, no band) to its last point; null elsewhere. `chargeSec` is CHARGE on the scope's time
 * base (the prediction's times are from CHARGE).
 */
export function predictionOnGrid(
	p: Prediction | null,
	x: number[],
	chargeSec: number
): PredictionOnGrid {
	const n = x.length;
	const out: PredictionOnGrid = {
		mid: new Array(n).fill(null),
		low: new Array(n).fill(null),
		high: new Array(n).fill(null)
	};
	if (!p || p.points.length === 0) return out;
	const pts = [{ t: p.t, bt: p.bt, low: p.bt, high: p.bt }, ...p.points].map((q) => ({
		...q,
		t: q.t + chargeSec
	}));
	const first = pts[0].t;
	const last = pts[pts.length - 1].t;
	let j = 0;
	for (let i = 0; i < n; i++) {
		const t = x[i];
		if (t < first - 0.5 || t > last + 1e-9) continue;
		while (j + 1 < pts.length - 1 && pts[j + 1].t < t) j++;
		const a = pts[j];
		const b = pts[Math.min(j + 1, pts.length - 1)];
		const w = b.t > a.t ? Math.min(1, Math.max(0, (t - a.t) / (b.t - a.t))) : 0;
		out.mid[i] = a.bt + (b.bt - a.bt) * w;
		out.low[i] = a.low + (b.low - a.low) * w;
		out.high[i] = a.high + (b.high - a.high) * w;
	}
	return out;
}

const clock = (t: number) => {
	const s = Math.max(0, Math.round(t));
	return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};

/** "7:31 (7:20–7:45)": a predicted landmark's time and range, from CHARGE. */
export function landmarkText(
	l: { t: number; low: number; high: number } | null | undefined
): string | null {
	if (!l) return null;
	return l.high - l.low >= 2 ? `${clock(l.t)} (${clock(l.low)}–${clock(l.high)})` : clock(l.t);
}

const KIND: Record<string, string> = { crash: 'RoR crash', flick: 'RoR flick', stall: 'BT stall' };

/** A live RoR warning in a few words: "BT stall ~9:05", "RoR flick now". */
export function rorWarningText(w: RorWarning): string {
	return `${KIND[w.kind] ?? w.kind} ${w.seen ? 'now' : `~${clock(w.t)}`}`;
}

/**
 * A roast's RoR warnings against what its RoR did: "flick warned 7:35, came 8:40 (65 s
 * ahead); stall at 9:20, not warned". Null when there were none of either.
 */
export function rorScoreText(s: Pick<RoastScore, 'ror_warnings' | 'ror_events'>): string | null {
	const warned = s.ror_warnings ?? [];
	const parts = warned.map((w) => {
		const k = KIND[w.kind] ?? w.kind;
		const head = w.seen ? `${k} seen ${clock(w.warned_at)}` : `${k} warned ${clock(w.warned_at)}`;
		if (w.came_t == null) return `${head}, did not come`;
		return w.lead_s != null && w.lead_s > 0
			? `${head}, came ${clock(w.came_t)} (${Math.round(w.lead_s)} s ahead)`
			: `${head}, came ${clock(w.came_t)}`;
	});
	for (const e of s.ror_events ?? []) {
		if (!warned.some((w) => w.kind === e.kind))
			parts.push(`${KIND[e.kind] ?? e.kind} at ${clock(e.t)}, not warned`);
	}
	return parts.length ? parts.join('; ') : null;
}

/** The prediction's two-minute error against the projection's on one roast, or the pool. */
export function scoreText(
	s: Pick<RoastScore, 'model' | 'projection' | 'predictions'>
): string | null {
	const at = (h: { horizon_s: number; rmse: number }[]) => h.find((x) => x.horizon_s === 120)?.rmse;
	const m = at(s.model);
	if (m == null) return null;
	const j = at(s.projection);
	return (
		`2 min ahead RMS ${m.toFixed(1)} °C` +
		(j != null ? ` (projection ${j.toFixed(1)} °C)` : '') +
		`, ${s.predictions} predictions`
	);
}

/** A roaster model's lines for its panel. */
export function modelLines(v: ModelView): {
	fitted: string;
	quality: { what: string; p90: number; rmse: number }[];
	settles: string;
	live: string | null;
} {
	const f = v.fit;
	const fitted = f
		? `${f.roasts} roasts, fitted ${f.fitted_at.slice(0, 16).replace('T', ' ')}; heater lag ${f.model.tau_s.toFixed(0)} s`
		: "The virtual roaster's model: no completed roast with a DROP here yet";
	const WHAT: Record<string, string> = {
		recorded: 'Heater and fan as they were',
		plan: 'The PID following the plan',
		held: 'Heater and fan held',
		projection: "Today's projection"
	};
	const quality = (f?.quality.modes ?? [])
		.map((m) => ({ mode: m.mode, h: m.horizons.find((h) => h.horizon_s === 120) }))
		.filter((m) => m.h)
		.map((m) => ({ what: WHAT[m.mode] ?? m.mode, p90: m.h!.p90, rmse: m.h!.rmse }));
	const settles = v.settles
		.map(
			(s) =>
				`${s.heater_pct}/${s.fan_pct} % → ${s.bt != null ? `${Math.round(s.bt)} °C` : 'over 300 °C'}`
		)
		.join(' · ');
	const p = v.pooled;
	const live =
		p.model_120 && p.projection_120
			? `Live on the last ${p.roasts} roasts: RMS ${p.model_120.rmse.toFixed(1)} °C at 2 min against the projection's ${p.projection_120.rmse.toFixed(1)} °C${p.beats_projection ? ': it beats it' : ': not yet better'}`
			: null;
	return { fitted, quality, settles, live };
}
