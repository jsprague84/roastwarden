/**
 * The live scope's axes for its width. On a phone three y axes (°C, RoR, %) took about 40 % of
 * the chart, so below COMPACT_SCOPE_PX the axes are narrower, carry no unit labels (the
 * readouts and the legend say the units) and the 0–100 % axis is left out: heater and fan keep
 * their fixed 0–100 scale and their values show in the readouts and on hover. Wider charts are
 * as before.
 */
export const COMPACT_SCOPE_PX = 560;

export interface AxisLayout {
	compact: boolean;
	/** Tick font, px */
	font: number;
	/** Unit labels beside the axes */
	labels: boolean;
	/** Axis widths, px; pct null = not drawn */
	temp: number;
	ror: number;
	pct: number | null;
	channel: number;
	/** Least px between time ticks */
	xSpace: number;
}

export function axisLayout(width: number): AxisLayout {
	if (width >= COMPACT_SCOPE_PX) {
		return {
			compact: false,
			font: 11,
			labels: true,
			temp: 52,
			ror: 52,
			pct: 44,
			channel: 44,
			xSpace: 70
		};
	}
	return {
		compact: true,
		font: 10,
		labels: false,
		temp: 36,
		ror: 30,
		pct: null,
		channel: 30,
		xSpace: 56
	};
}

/** The px the y axes take, for a check that compact frees room. */
export function yAxesWidth(l: AxisLayout, channels = 0): number {
	return l.temp + l.ror + (l.pct ?? 0) + channels * l.channel;
}

/**
 * Time steps, s, that read as clock times: 2:00 rather than uPlot's and ECharts' 1-2-5 steps
 * (100 s = 1:40, 200 s = 3:20). uPlot takes it as the x axis's `incrs`.
 */
export const TIME_INCRS = [
	1, 2, 5, 10, 15, 30, 60, 120, 180, 300, 600, 900, 1200, 1800, 3600, 7200
];

/** The smallest clock step that puts at most `maxTicks` ticks over `spanSec` (ECharts' `interval`). */
export function clockStep(spanSec: number, maxTicks: number): number {
	const n = Math.max(1, Math.floor(maxTicks));
	return TIME_INCRS.find((s) => spanSec / s <= n) ?? TIME_INCRS[TIME_INCRS.length - 1];
}
