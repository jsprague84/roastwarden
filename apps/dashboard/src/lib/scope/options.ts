/**
 * The live scope's uPlot options: the scales (temperature, RoR clipped at the charge drop,
 * percent, mirrors for the projections, one per declared channel in a unit of its own), the
 * axes for the width, the series in the model's layout, the bands between them, and the hooks
 * the component draws and zooms with.
 */
import uPlot, { type Options, type Series } from 'uplot';
import type { ChartPalette } from '#lib/constants/chartTheme.js';
import { channelColor } from '#lib/roaster/channels.js';
import { deltaToDisplay, rorUnit, tempUnit } from '#lib/stores/units.svelte.js';
import { TIME_INCRS, axisLayout } from './axes.js';
import { SERIES_COLORS, SERIES_LABELS } from './draw.js';
import { BAND, MODEL_HI, MODEL_LO, SERIES, type SeriesKey } from './model.js';
import { fmtClock, type ScopeChannel } from './scopeData.js';

export interface ScopeOptionsInput {
	width: number;
	height: number;
	pal: ChartPalette;
	channels: ScopeChannel[];
	visible: Record<SeriesKey, boolean>;
	channelShown: (key: string) => boolean;
	/** The x range for the data's extent: the view's, or the chart's own while zoomed. */
	xRange: (u: uPlot, min: number, max: number) => [number, number];
	/** Top of a channel's own scale, by channel index; null: its max, else 1. */
	channelTop: (i: number) => number | null;
	hooks: {
		drawClear: (u: uPlot) => void;
		draw: (u: uPlot) => void;
		setCursor: (u: uPlot) => void;
		/** A drag selection zoomed the chart. */
		onZoomSelect: () => void;
	};
}

export function scopeOptions(o: ScopeOptionsInput): Options {
	const { pal, channels, visible } = o;
	const tu = tempUnit();
	const ru = rorUnit();
	const grid = { stroke: pal.line, width: 1 };
	const ticks = { stroke: pal.line, width: 1 };
	const lay = axisLayout(o.width);
	const font = `${lay.font}px ${pal.font}`;
	const label = (l: string) =>
		lay.labels ? { label: l, labelFont: font, labelSize: 14 } : { labelSize: 0 };
	const axisStroke = pal.axisText;

	function seriesDef(k: SeriesKey, scale: string, extra: Partial<Series> = {}): Series {
		return {
			label: SERIES_LABELS[k],
			scale,
			stroke: SERIES_COLORS[k],
			width: 2,
			points: { show: false },
			spanGaps: false,
			show: visible[k],
			...extra
		};
	}

	/** Channels in another unit, each on a scale and right-hand axis of its own. */
	const ownChannels = channels
		.map((c, i) => ({ c, i }))
		.filter(({ c }) => c.unit !== 'C' && c.unit !== '%');

	return {
		width: o.width,
		height: o.height,
		padding: [40, 8, 0, 0],
		cursor: {
			drag: { x: true, y: false, setScale: true },
			points: { size: 7 },
			focus: { prox: 24 }
		},
		legend: { show: false },
		scales: {
			x: {
				time: false,
				range: (u, min, max) => o.xRange(u, min, max)
			},
			temp: {
				range: (u, min, max) => {
					// Pad to round tens; keep at least a 50° span so an idle roaster is not a flat zoom.
					const lo = Math.floor((min - 10) / 10) * 10;
					const hi = Math.ceil((max + 10) / 10) * 10;
					return [lo, Math.max(hi, lo + 50)];
				}
			},
			ror: {
				range: (u, min, max) => {
					const floor = deltaToDisplay(-5);
					const ceil = deltaToDisplay(25);
					const cap = deltaToDisplay(60);
					// Clip the charge drop (-200 °C/min is normal for a few seconds) so the
					// axis stays readable; the line simply leaves the plot.
					const lo = Math.max(
						deltaToDisplay(-30),
						Math.min(floor, Math.floor(Math.min(min, 0) / 5) * 5)
					);
					const hi = Math.max(ceil, Math.ceil(Math.min(Math.max(max, 0), cap) / 5) * 5);
					return [lo, hi];
				}
			},
			pct: { range: [0, 100] },
			// Projections ride on mirrors of the live scales so an optimistic
			// extrapolation never stretches the axis the roast is drawn on.
			tempProj: { from: 'temp', range: (u) => [u.scales.temp.min ?? 0, u.scales.temp.max ?? 1] },
			rorProj: { from: 'ror', range: (u) => [u.scales.ror.min ?? 0, u.scales.ror.max ?? 1] },
			// A channel in another unit: 0 to its top (its max, else the highest value seen)
			...Object.fromEntries(
				ownChannels.map(({ c, i }) => [
					`ch:${c.key}`,
					{ range: (): [number, number] => [0, o.channelTop(i) ?? c.max ?? 1] }
				])
			)
		},
		axes: [
			{
				stroke: axisStroke,
				grid,
				ticks,
				font,
				values: (u, vals) => vals.map(fmtClock),
				space: lay.xSpace,
				incrs: TIME_INCRS
			},
			{ scale: 'temp', ...label(tu), stroke: axisStroke, grid, ticks, font, size: lay.temp },
			{
				scale: 'ror',
				side: 1,
				...label(ru),
				stroke: SERIES_COLORS.rorBt,
				grid: { show: false },
				ticks,
				font,
				size: lay.ror
			},
			{
				scale: 'pct',
				side: 1,
				...label('%'),
				stroke: pal.mutedText,
				grid: { show: false },
				ticks,
				font,
				size: lay.pct ?? 0,
				show: lay.pct !== null
			},
			...ownChannels.map(({ c, i }) => ({
				scale: `ch:${c.key}`,
				side: 1,
				...label(c.unit),
				stroke: channelColor(i),
				grid: { show: false },
				ticks,
				font,
				size: lay.channel
			}))
		],
		series: [
			{ label: 'Time', value: (u, v) => (v == null ? '' : fmtClock(v)) },
			seriesDef('bt', 'temp', { width: 2.5 }),
			seriesDef('et', 'temp'),
			seriesDef('sp', 'temp', { width: 1.5, dash: [6, 4] }),
			seriesDef('rorBt', 'ror'),
			seriesDef('rorEt', 'ror', { width: 1.5 }),
			seriesDef('heater', 'pct', { width: 1.5, paths: uPlot.paths.stepped!({ align: 1 }) }),
			seriesDef('fan', 'pct', { width: 1.5, paths: uPlot.paths.stepped!({ align: 1 }) }),
			seriesDef('profBt', 'temp', { width: 1.5, dash: [4, 4] }),
			seriesDef('profEt', 'temp', { width: 1, dash: [4, 4] }),
			seriesDef('projBt', 'tempProj', { width: 2, dash: [2, 4] }),
			seriesDef('projRor', 'rorProj', { width: 1.5, dash: [2, 4] }),
			{
				label: 'Δ band',
				scale: 'temp',
				stroke: 'transparent',
				width: 0,
				points: { show: false },
				spanGaps: false,
				show: visible.profBt
			},
			seriesDef('profRor', 'ror', { width: 1.5, dash: [4, 4] }),
			seriesDef('model', 'tempProj', { width: 2 }),
			{
				label: 'Model low',
				scale: 'tempProj',
				stroke: 'transparent',
				width: 0,
				points: { show: false },
				spanGaps: false,
				show: visible.model
			},
			{
				label: 'Model high',
				scale: 'tempProj',
				stroke: 'transparent',
				width: 0,
				points: { show: false },
				spanGaps: false,
				show: visible.model
			},
			...channels.map((c, i) => ({
				label: c.label,
				scale: c.unit === 'C' ? 'temp' : c.unit === '%' ? 'pct' : `ch:${c.key}`,
				stroke: channelColor(i),
				width: 1.5,
				dash: [3, 3],
				points: { show: false },
				spanGaps: false,
				show: o.channelShown(c.key)
			}))
		],
		bands: [
			{ series: [SERIES.bt, BAND], fill: 'rgba(248, 113, 113, 0.18)', dir: 1 },
			{ series: [BAND, SERIES.bt], fill: 'rgba(52, 211, 153, 0.18)', dir: 1 },
			{ series: [MODEL_HI, MODEL_LO], fill: 'rgba(232, 121, 249, 0.16)' }
		],
		hooks: {
			drawClear: [o.hooks.drawClear],
			draw: [o.hooks.draw],
			setCursor: [o.hooks.setCursor],
			setSelect: [
				(u) => {
					if (u.select.width > 0) o.hooks.onZoomSelect();
				}
			]
		}
	};
}
