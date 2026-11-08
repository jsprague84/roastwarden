/**
 * The profile designer's chart (ECharts): the BT and ET splines through the points, the
 * simulated RoR, fan and heater, the recorded background curves, the events and phase marks,
 * and the draggable handles drawn over the points. Data is in the display unit (the points
 * stay °C).
 */
import type { GraphicComponentOption } from 'echarts/components';
import type { ECharts } from 'echarts/core';
import type { ProfileCurves, ProfileEventInput } from '#lib/api/profiles.js';
import type { ECOption } from '#lib/components/ui/Chart.svelte';
import type { ChartPalette } from '#lib/constants/chartTheme.js';
import { describeEvent, eventTimeOnCurve } from '#lib/profile/events.js';
import {
	deltaToDisplay,
	fromDisplay,
	rorUnit,
	tempUnit,
	toDisplay
} from '#lib/stores/units.svelte.js';
import {
	evaluateDerivativeGrid,
	evaluateSplineGrid,
	fitNaturalCubicSpline
} from '#lib/utils/spline.js';
import {
	DRY_TEMP,
	FC_TEMP,
	axisRange,
	findTempCrossing,
	fmtTempLabel,
	indexOfPoint,
	landmarkTime,
	type DesignerPoint
} from './designer.js';

const SYMBOL_SIZE = 14;
const HEATER_COLOR = '#f97316';
const FAN_COLOR = '#a78bfa';

/** The chart's grid, pixels from each edge; the handles' pixel mapping uses the same. */
const GRID = { left: 55, right: 55, top: 40, bottom: 45 };

export interface Axes {
	x: { min: number; max: number };
	y: { min: number; max: number };
}

/** The axis ranges for the points (display unit): 12 minutes and 250 °C for an empty chart. */
export function designerAxes(sorted: DesignerPoint[]): Axes {
	const times = sorted.map((p) => p.time_seconds);
	const temps = sorted.map((p) => toDisplay(p.target_temp));
	return {
		x: axisRange(times, 720, 30),
		y: axisRange(temps, Math.ceil(toDisplay(250)), Math.ceil(deltaToDisplay(20)))
	};
}

/** Data → pixel for the axis ranges about to be applied (the chart's own may be stale),
 * matching the grid offsets of the chart option exactly. */
export function pixelMapper(
	width: number,
	height: number,
	axes: Axes
): (dataX: number, dataY: number) => [number, number] {
	const pw = width - GRID.left - GRID.right;
	const ph = height - GRID.top - GRID.bottom;
	const xSpan = axes.x.max - axes.x.min || 1;
	const ySpan = axes.y.max - axes.y.min || 1;
	return (dataX, dataY) => [
		GRID.left + ((dataX - axes.x.min) / xSpan) * pw,
		GRID.top + (1 - (dataY - axes.y.min) / ySpan) * ph
	];
}

/** What the handles report back, by the point's index in the unsorted list. */
export interface HandleEvents {
	/** The BT handle is being dragged: the point's new time and BT (°C), or null when the
	 * pixel maps to nothing (the drag still counts, so the click that ends it adds no point). */
	onDragBt(index: number, to: [timeSec: number, tempC: number] | null): void;
	/** The ET handle is being dragged: the point's new ET (°C), or null as above. */
	onDragEt(index: number, tempC: number | null): void;
	/** A drag ended on the point. */
	onDragEnd(index: number): void;
	onSelect(index: number | null): void;
}

/** The draggable circles over each point: a BT handle (amber), and an ET handle (blue) for a
 * point with an ET value; the selected point's are red. */
export function handleGraphics(
	instance: ECharts,
	points: DesignerPoint[],
	selectedIndex: number | null,
	axes: Axes,
	pal: ChartPalette,
	on: HandleEvents
): GraphicComponentOption[] {
	const toPixel = pixelMapper(instance.getWidth(), instance.getHeight(), axes);
	const sorted = [...points].sort((a, b) => a.time_seconds - b.time_seconds);
	const elements: GraphicComponentOption[] = [];

	for (const pt of sorted) {
		// Map back to the real index in the unsorted `points` array
		const realIdx = indexOfPoint(points, pt);
		const isSelected = selectedIndex === realIdx;

		const [px, py] = toPixel(pt.time_seconds, toDisplay(pt.target_temp));
		elements.push({
			type: 'circle',
			x: px,
			y: py,
			shape: { r: SYMBOL_SIZE / 2 },
			style: {
				fill: isSelected ? '#ef4444' : '#f59e0b',
				stroke: isSelected ? pal.handleStroke : pal.handleStrokeSoft,
				lineWidth: isSelected ? 3 : 1
			},
			draggable: true,
			z: 100,
			cursor: 'move',
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			ondrag(this: any) {
				const newVal = instance.convertFromPixel('grid', [this.x, this.y] as [number, number]);
				if (!newVal || realIdx < 0) {
					on.onDragBt(realIdx, null);
					return;
				}
				const [newTime, newTempDisp] = newVal as number[];
				on.onDragBt(realIdx, [
					Math.max(0, Math.round(newTime)),
					Math.max(0, Math.round(fromDisplay(newTempDisp) * 10) / 10)
				]);
			},
			ondragend() {
				on.onDragEnd(realIdx);
			},
			onclick() {
				on.onSelect(realIdx >= 0 ? realIdx : null);
			}
		});

		if (pt.target_env_temp !== null) {
			const [epx, epy] = toPixel(pt.time_seconds, toDisplay(pt.target_env_temp));
			elements.push({
				type: 'circle',
				x: epx,
				y: epy,
				shape: { r: SYMBOL_SIZE / 2 - 1 },
				style: {
					fill: isSelected ? '#ef4444' : '#60a5fa',
					stroke: isSelected ? pal.handleStroke : pal.handleStrokeSoft,
					lineWidth: isSelected ? 3 : 1
				},
				draggable: 'vertical' as unknown as boolean,
				z: 99,
				cursor: 'ns-resize',
				// eslint-disable-next-line @typescript-eslint/no-explicit-any
				ondrag(this: any) {
					const newVal = instance.convertFromPixel('grid', [this.x, this.y] as [number, number]);
					if (!newVal || realIdx < 0) {
						on.onDragEt(realIdx, null);
						return;
					}
					const [, newTempDisp] = newVal as number[];
					on.onDragEt(realIdx, Math.max(0, Math.round(fromDisplay(newTempDisp) * 10) / 10));
				},
				onclick() {
					on.onSelect(realIdx >= 0 ? realIdx : null);
				}
			});
		}
	}

	return elements;
}

const clock = (val: number) => {
	const m = Math.floor(val / 60);
	const s = Math.round(val % 60);
	return `${m}:${s.toString().padStart(2, '0')}`;
};

/** The chart option for the points and events; `graphic` is the handles drawn over it. */
export function designerChartOption(o: {
	sorted: DesignerPoint[];
	events: ProfileEventInput[];
	/** Recorded background curves (read-only), when the profile came from a roast. */
	curves: ProfileCurves | null;
	/** False once the owner asked for the curves to be removed on save. */
	showCurves: boolean;
	axes: Axes;
	pal: ChartPalette;
	graphic: GraphicComponentOption[];
}): ECOption {
	const { sorted, events, curves, pal, axes } = o;
	const maxTime = sorted.length > 0 ? sorted[sorted.length - 1].time_seconds : 0;

	// Fit cubic spline for BT curve
	const btSpline = fitNaturalCubicSpline(
		sorted.map((p) => ({ x: p.time_seconds, y: p.target_temp }))
	);
	const btCurveData: [number, number][] = (
		sorted.length >= 2
			? evaluateSplineGrid(btSpline, 0, maxTime, 2)
			: sorted.map((p): [number, number] => [p.time_seconds, p.target_temp])
	).map(([t, y]) => [t, toDisplay(y)]);

	// Compute RoR from spline derivative (°C/s → display unit per minute)
	const rorData: [number, number][] =
		sorted.length >= 2
			? evaluateDerivativeGrid(btSpline, 0, maxTime, 5).map(([t, dy]) => [
					t,
					Math.round(deltaToDisplay(dy * 60) * 10) / 10
				])
			: [];
	const rorValues = rorData.map(([, v]) => v);
	const rorMax = rorValues.length > 0 ? Math.max(...rorValues, 5) : 30;
	const rorMin = rorValues.length > 0 ? Math.min(...rorValues, 0) : 0;

	// ET (Environment Temp) spline — only when ET data exists
	const etPoints = sorted.filter((p) => p.target_env_temp !== null);
	const hasEtData = etPoints.length >= 2;
	let etCurveData: [number, number][] = [];
	if (hasEtData) {
		const etSpline = fitNaturalCubicSpline(
			etPoints.map((p) => ({ x: p.time_seconds, y: p.target_env_temp! }))
		);
		etCurveData = evaluateSplineGrid(etSpline, 0, maxTime, 2).map(([t, y]) => [t, toDisplay(y)]);
	}

	// Fan and heater feedforward (linear between points, like the replay will use)
	const fanData = sorted
		.filter((p) => p.fan_speed !== null)
		.map((p) => [p.time_seconds, p.fan_speed as number]);
	const hasFanData = fanData.length > 0;
	const heaterData = sorted
		.filter((p) => p.heater_pct !== null)
		.map((p) => [p.time_seconds, p.heater_pct as number]);
	const hasHeaterData = heaterData.length > 0;
	const hasPct = hasFanData || hasHeaterData;

	// Recorded background curves (read-only), when the profile came from a roast
	const showCurves = curves != null && o.showCurves;
	const curveSeries = (arr: (number | null)[] | undefined, conv: (v: number) => number) =>
		(arr ?? []).map((v, i) => [i * (curves?.interval_seconds ?? 1), v == null ? null : conv(v)]);
	const recBt = showCurves ? curveSeries(curves?.bt, toDisplay) : [];
	const recEt = showCurves ? curveSeries(curves?.et, toDisplay) : [];

	// Right Y-axis range: use 0-100 when fan/heater data exists, otherwise fit RoR
	const rightAxisMax = hasPct ? 100 : Math.ceil(rorMax + 2);
	const rightAxisMin = hasPct ? 0 : Math.floor(rorMin);

	// Events as flags along the chart
	const curveC = sorted.map((p): [number, number] => [p.time_seconds, p.target_temp]);
	const eventMarks = events
		.map((e) => ({ e, t: eventTimeOnCurve(e, curveC) }))
		.filter((x): x is { e: ProfileEventInput; t: number } => x.t != null)
		.map(({ e, t }) => ({
			xAxis: t,
			label: {
				formatter: (e.trigger_kind === 'bt' ? '⟂ ' : '') + describeEvent(e, fmtTempLabel),
				position: 'insideEndBottom' as const
			},
			lineStyle: {
				color: e.action === 'heater' ? HEATER_COLOR : e.action === 'fan' ? FAN_COLOR : pal.mutedText
			}
		}));

	// Phase markers estimated from BT thresholds, unless landmark events already mark them
	const dryTime =
		landmarkTime(events, curveC, 'drying_end') == null ? findTempCrossing(sorted, DRY_TEMP) : null;
	const fcTime =
		landmarkTime(events, curveC, 'first_crack_start') == null
			? findTempCrossing(sorted, FC_TEMP)
			: null;

	const markLineData: {
		xAxis: number;
		label?: { formatter: string; position: 'insideEndTop' };
		lineStyle?: { color: string };
	}[] = [];
	if (dryTime !== null) {
		markLineData.push({
			xAxis: dryTime,
			label: { formatter: 'DRY', position: 'insideEndTop' },
			lineStyle: { color: '#22c55e' }
		});
	}
	if (fcTime !== null) {
		markLineData.push({
			xAxis: fcTime,
			label: { formatter: 'FC', position: 'insideEndTop' },
			lineStyle: { color: '#eab308' }
		});
	}

	return {
		animation: false,
		grid: { left: GRID.left, right: GRID.right, top: GRID.top, bottom: GRID.bottom },
		legend: {
			show: true,
			data: ['BT Profile', 'ET Profile', 'Simulated RoR', 'Fan Speed', 'Heater', 'Recorded BT'],
			top: 5,
			textStyle: { color: pal.axisText, fontSize: 11 }
		},
		tooltip: {
			trigger: 'item',
			formatter: (params: unknown) => {
				const p = params as { data?: number[]; seriesName?: string };
				if (!p.data) return '';
				const [t, val] = p.data;
				const m = Math.floor(t / 60);
				const s = Math.round(t % 60);
				const unit =
					p.seriesName === 'Simulated RoR'
						? rorUnit()
						: p.seriesName === 'Fan Speed' || p.seriesName === 'Heater'
							? '%'
							: tempUnit();
				return `Time: ${m}:${s.toString().padStart(2, '0')}<br/>${p.seriesName}: ${val.toFixed(1)}${unit}`;
			}
		},
		xAxis: {
			type: 'value',
			name: 'Time (s)',
			min: axes.x.min,
			max: axes.x.max,
			nameTextStyle: { color: pal.axisText },
			axisLabel: { color: pal.axisText, formatter: clock },
			axisLine: { lineStyle: { color: pal.line } },
			splitLine: { show: false }
		},
		yAxis: [
			{
				type: 'value',
				name: `Temp (${tempUnit()})`,
				min: axes.y.min,
				max: axes.y.max,
				nameTextStyle: { color: pal.axisText },
				axisLabel: { color: pal.axisText },
				axisLine: { lineStyle: { color: pal.line } },
				splitLine: { lineStyle: { color: pal.lineFaint } }
			},
			{
				type: 'value',
				name: hasPct ? 'RoR / %' : `RoR (${rorUnit()})`,
				min: rightAxisMin,
				max: rightAxisMax,
				nameTextStyle: { color: '#34d399' },
				axisLabel: { color: '#34d399' },
				axisLine: { lineStyle: { color: '#34d399' } },
				splitLine: { show: false }
			}
		],
		series: [
			{
				id: 'profile',
				name: 'BT Profile',
				type: 'line',
				data: btCurveData,
				itemStyle: { color: '#f59e0b' },
				lineStyle: { width: 2 },
				showSymbol: false,
				symbolSize: 0,
				markLine:
					markLineData.length > 0
						? {
								silent: true,
								symbol: 'none',
								lineStyle: { type: 'dashed', width: 1 },
								label: { color: pal.axisText, fontSize: 10 },
								data: markLineData
							}
						: undefined
			},
			...(hasEtData
				? [
						{
							id: 'et',
							name: 'ET Profile',
							type: 'line' as const,
							data: etCurveData,
							itemStyle: { color: '#60a5fa' },
							lineStyle: { width: 1.5, type: 'dashed' as const, color: '#60a5fa' },
							showSymbol: false,
							symbolSize: 0,
							silent: true
						}
					]
				: []),
			{
				id: 'ror',
				name: 'Simulated RoR',
				type: 'line',
				data: rorData,
				yAxisIndex: 1,
				itemStyle: { color: '#34d399' },
				lineStyle: { width: 1.5, type: 'dashed', color: '#34d399' },
				showSymbol: false,
				symbolSize: 0,
				silent: true
			},
			...(hasFanData
				? [
						{
							id: 'fan',
							name: 'Fan Speed',
							type: 'line' as const,
							data: fanData,
							yAxisIndex: 1,
							itemStyle: { color: FAN_COLOR },
							lineStyle: { width: 1.5, color: FAN_COLOR },
							areaStyle: { opacity: 0.12, color: FAN_COLOR },
							showSymbol: false,
							symbolSize: 0,
							silent: true
						}
					]
				: []),
			...(hasHeaterData
				? [
						{
							id: 'heater',
							name: 'Heater',
							type: 'line' as const,
							data: heaterData,
							yAxisIndex: 1,
							itemStyle: { color: HEATER_COLOR },
							lineStyle: { width: 1.5, color: HEATER_COLOR },
							showSymbol: false,
							symbolSize: 0,
							silent: true
						}
					]
				: []),
			...(recBt.length
				? [
						{
							id: 'rec-bt',
							name: 'Recorded BT',
							type: 'line' as const,
							data: recBt,
							itemStyle: { color: '#f59e0b' },
							lineStyle: { width: 4, opacity: 0.2, color: '#f59e0b' },
							showSymbol: false,
							symbolSize: 0,
							silent: true,
							z: 1
						}
					]
				: []),
			...(recEt.length
				? [
						{
							id: 'rec-et',
							name: 'Recorded ET',
							type: 'line' as const,
							data: recEt,
							itemStyle: { color: '#60a5fa' },
							lineStyle: { width: 4, opacity: 0.15, color: '#60a5fa' },
							showSymbol: false,
							symbolSize: 0,
							silent: true,
							z: 1
						}
					]
				: []),
			...(eventMarks.length
				? [
						{
							id: 'events',
							name: 'Events',
							type: 'line' as const,
							data: [],
							silent: true,
							markLine: {
								silent: true,
								symbol: 'none',
								lineStyle: { type: 'dotted' as const, width: 1 },
								label: { color: pal.axisText, fontSize: 9 },
								data: eventMarks
							}
						}
					]
				: [])
		],
		graphic: o.graphic
	};
}
