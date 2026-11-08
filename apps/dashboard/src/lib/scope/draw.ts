/**
 * The scope's series colours and its canvas drawing that needs no store: phase bands behind the
 * curves, landmarks as dashed lines with labels above the plot. LiveScope and roastwarden.com's
 * roast demo (src/demo) both draw with these, so the demo looks like the app.
 */
import type uPlot from 'uplot';
import type { ChartPalette } from '#lib/constants/chartPalettes.js';
import { landmarkColors, eventLabels } from '#lib/constants/landmarks.js';
import { fmtClock, stackLabels, type PhaseBand, type PhaseKind } from './scopeData.js';

export const SERIES_COLORS = {
	bt: '#f59e0b',
	et: '#60a5fa',
	sp: '#f97316',
	rorBt: '#34d399',
	rorEt: '#2dd4bf',
	heater: '#f87171',
	fan: '#a78bfa',
	profBt: '#fbbf24',
	profEt: '#93c5fd',
	projBt: '#f59e0b',
	projRor: '#34d399',
	profRor: '#86efac',
	model: '#e879f9'
} as const;

export type ScopeSeriesKey = keyof typeof SERIES_COLORS;

export const SERIES_LABELS: Record<ScopeSeriesKey, string> = {
	bt: 'BT',
	et: 'ET',
	sp: 'SP',
	rorBt: 'RoR BT',
	rorEt: 'RoR ET',
	heater: 'Heater',
	fan: 'Fan',
	profBt: 'Profile BT',
	profEt: 'Profile ET',
	projBt: 'BT →',
	projRor: 'RoR →',
	profRor: 'Profile RoR',
	model: 'BT model'
};

const PHASE_FILL: Record<PhaseKind, string> = {
	drying: 'rgba(234, 179, 8, 0.07)',
	maillard: 'rgba(249, 115, 22, 0.08)',
	development: 'rgba(239, 68, 68, 0.08)',
	cooling: 'rgba(96, 165, 250, 0.07)'
};

/** A landmark on the scope's time base. */
export interface ScopeMark {
	t: number;
	type: string;
	/** °C, or null */
	temp: number | null;
}

/** Phase bands behind the curves, each labelled with its length and share when it has room. */
export function drawPhaseBands(
	u: uPlot,
	bands: PhaseBand[],
	pct: Record<PhaseKind, number>,
	pal: ChartPalette
): void {
	if (bands.length === 0) return;
	const { ctx, bbox } = u;
	const dpr = devicePixelRatio;
	ctx.save();
	for (const b of bands) {
		const x0 = Math.max(bbox.left, u.valToPos(b.from, 'x', true));
		const x1 = Math.min(bbox.left + bbox.width, u.valToPos(b.to, 'x', true));
		if (x1 <= x0) continue;
		ctx.fillStyle = PHASE_FILL[b.kind];
		ctx.fillRect(x0, bbox.top, x1 - x0, bbox.height);
		ctx.fillStyle = pal.mutedText;
		ctx.font = `${11 * dpr}px ${pal.font}`;
		ctx.textBaseline = 'top';
		ctx.textAlign = 'left';
		const p = pct[b.kind];
		const label =
			b.kind === 'cooling'
				? 'Cooling'
				: `${b.label} ${fmtClock(b.to - b.from)}${p ? ` · ${p}%` : ''}`;
		if (x1 - x0 > 60 * dpr) ctx.fillText(label, x0 + 6 * dpr, bbox.top + 4 * dpr);
	}
	ctx.restore();
}

/**
 * Landmarks: dashed verticals with labels above the plot. Labels of landmarks close together
 * stack in up to three rows; without room the temperature goes, then the label (the dashed
 * line stays). `tempText` formats a °C value for display (the unit setting). Call inside a clip
 * that reaches 38 px above the plot.
 */
export function drawLandmarks(
	u: uPlot,
	marks: ScopeMark[],
	pal: ChartPalette,
	tempText: (c: number) => string
): void {
	const { ctx, bbox } = u;
	const dpr = devicePixelRatio;
	ctx.font = `${10 * dpr}px ${pal.font}`;
	ctx.textBaseline = 'bottom';
	ctx.textAlign = 'center';
	const shown = [...marks]
		.sort((a, b) => a.t - b.t)
		.map((mk) => ({ mk, x: u.valToPos(mk.t, 'x', true) }))
		.filter(({ x }) => x >= bbox.left && x <= bbox.left + bbox.width)
		.map(({ mk, x }) => {
			const label = eventLabels[mk.type] ?? mk.type;
			const texts = mk.temp != null ? [`${label} ${tempText(mk.temp)}°`, label] : [label];
			return { mk, x, texts, widths: texts.map((t) => ctx.measureText(t).width) };
		});
	const placed = stackLabels(shown, 3, bbox.left, bbox.left + bbox.width, 6 * dpr);
	ctx.textAlign = 'left';
	shown.forEach(({ mk, x, texts }, i) => {
		ctx.strokeStyle = landmarkColors[mk.type] ?? pal.mutedText;
		ctx.lineWidth = 1 * dpr;
		ctx.setLineDash([4 * dpr, 4 * dpr]);
		ctx.beginPath();
		ctx.moveTo(x, bbox.top);
		ctx.lineTo(x, bbox.top + bbox.height);
		ctx.stroke();
		ctx.setLineDash([]);
		const p = placed[i];
		if (!p) return;
		ctx.fillStyle = landmarkColors[mk.type] ?? pal.labelText;
		ctx.fillText(texts[p.choice], p.left, bbox.top - (4 + p.row * 11) * dpr);
	});
}
