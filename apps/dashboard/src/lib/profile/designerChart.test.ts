import { describe, expect, it, vi } from 'vitest';
import type { ECharts } from 'echarts/core';
import type { ProfileEventInput } from '#lib/api/profiles.js';
import { chartPalette } from '#lib/constants/chartTheme.js';
import { blankEvent } from '#lib/profile/events.js';
import type { DesignerPoint } from './designer.js';
import { designerAxes, designerChartOption, handleGraphics, pixelMapper } from './designerChart.js';

const pt = (
	time_seconds: number,
	target_temp: number,
	extra: Partial<DesignerPoint> = {}
): DesignerPoint => ({
	time_seconds,
	target_temp,
	fan_speed: null,
	target_env_temp: null,
	heater_pct: null,
	notes: null,
	...extra
});

const CURVE = [pt(0, 180), pt(60, 95), pt(240, 150), pt(480, 200), pt(600, 215)];
const pal = chartPalette();

type Series = {
	id?: string;
	name?: string;
	data?: unknown[];
	markLine?: { data: { xAxis: number }[] };
};
const seriesOf = (o: ReturnType<typeof designerChartOption>) => o.series as Series[];

describe('the axes and the pixel mapping', () => {
	it('spans the points with padding, and an empty chart twelve minutes to 250 °C', () => {
		expect(designerAxes([])).toEqual({ x: { min: 0, max: 720 }, y: { min: 0, max: 250 } });
		expect(designerAxes(CURVE)).toEqual({ x: { min: 0, max: 630 }, y: { min: 75, max: 235 } });
	});

	it('maps data to the chart grid, the y axis upward', () => {
		const axes = { x: { min: 0, max: 600 }, y: { min: 100, max: 200 } };
		const toPixel = pixelMapper(710, 485, axes); // a 600 × 400 grid inside the margins
		expect(toPixel(0, 100)).toEqual([55, 440]);
		expect(toPixel(600, 200)).toEqual([655, 40]);
		expect(toPixel(300, 150)).toEqual([355, 240]);
	});
});

describe('the chart option', () => {
	it('draws the spline, the RoR, DRY and FC marks for a curve with no events', () => {
		const o = designerChartOption({
			sorted: CURVE,
			events: [],
			curves: null,
			showCurves: true,
			axes: designerAxes(CURVE),
			pal,
			graphic: []
		});
		const series = seriesOf(o);
		expect(series.map((s) => s.id)).toEqual(['profile', 'ror']);
		expect(series[0].data!.length).toBeGreaterThan(100);
		expect(series[0].markLine!.data.map((d) => d.xAxis)).toEqual([240, 480]);
		const yAxis = o.yAxis as { name: string; min: number; max: number }[];
		expect(yAxis[1].name).toMatch(/^RoR \(/);
	});

	it('adds ET, fan, heater, the recorded curves and the events when the profile has them', () => {
		const sorted = [
			pt(0, 180, { target_env_temp: 200, fan_speed: 70, heater_pct: 80 }),
			pt(60, 95),
			pt(300, 170, { target_env_temp: 210 }),
			pt(600, 215, { fan_speed: 50 })
		];
		const fc: ProfileEventInput = {
			...blankEvent(450),
			action: 'landmark',
			label: 'first_crack_start'
		};
		const o = designerChartOption({
			sorted,
			events: [fc, { ...blankEvent(60), action: 'fan', value: 60 }],
			curves: {
				source_session_id: null,
				interval_seconds: 2,
				bt: [180, 150, null],
				et: [200, 190, 180],
				ror: [],
				heater: [],
				fan: []
			},
			showCurves: true,
			axes: designerAxes(sorted),
			pal,
			graphic: []
		});
		const ids = seriesOf(o).map((s) => s.id);
		expect(ids).toEqual(['profile', 'et', 'ror', 'fan', 'heater', 'rec-bt', 'rec-et', 'events']);
		// the FC landmark replaces the 200 °C estimate; DRY (150 °C) is still estimated
		const profile = seriesOf(o)[0];
		expect(profile.markLine!.data.map((d) => d.xAxis)).toEqual([236]);
		const events = seriesOf(o).find((s) => s.id === 'events')!;
		expect(events.markLine!.data.map((d) => d.xAxis)).toEqual([450, 60]);
		const recBt = seriesOf(o).find((s) => s.id === 'rec-bt')!;
		expect(recBt.data).toEqual([
			[0, 180],
			[2, 150],
			[4, null]
		]);
		const yAxis = o.yAxis as { name: string; min: number; max: number }[];
		expect(yAxis[1]).toMatchObject({ name: 'RoR / %', min: 0, max: 100 });
	});

	it('leaves the recorded curves out once they are to be removed', () => {
		const o = designerChartOption({
			sorted: CURVE,
			events: [],
			curves: {
				source_session_id: null,
				interval_seconds: 1,
				bt: [180],
				et: [200],
				ror: [],
				heater: [],
				fan: []
			},
			showCurves: false,
			axes: designerAxes(CURVE),
			pal,
			graphic: []
		});
		expect(seriesOf(o).map((s) => s.id)).toEqual(['profile', 'ror']);
	});
});

describe('the tooltip and the axis labels', () => {
	it('names the series’ unit and writes the time as m:ss', () => {
		const o = designerChartOption({
			sorted: CURVE,
			events: [],
			curves: null,
			showCurves: true,
			axes: designerAxes(CURVE),
			pal,
			graphic: []
		});
		const tooltip = o.tooltip as { formatter: (p: unknown) => string };
		expect(tooltip.formatter({ data: [125, 180.26], seriesName: 'BT Profile' })).toBe(
			'Time: 2:05<br/>BT Profile: 180.3°C'
		);
		expect(tooltip.formatter({ data: [60, 12], seriesName: 'Simulated RoR' })).toBe(
			'Time: 1:00<br/>Simulated RoR: 12.0°C/min'
		);
		expect(tooltip.formatter({ data: [60, 70], seriesName: 'Fan Speed' })).toBe(
			'Time: 1:00<br/>Fan Speed: 70.0%'
		);
		expect(tooltip.formatter({})).toBe('');
		const xAxis = o.xAxis as { axisLabel: { formatter: (v: number) => string } };
		expect(xAxis.axisLabel.formatter(605)).toBe('10:05');
	});
});

describe('the handles', () => {
	const instance = {
		getWidth: () => 710,
		getHeight: () => 485,
		convertFromPixel: vi.fn()
	} as unknown as ECharts;
	const on = { onDragBt: vi.fn(), onDragEt: vi.fn(), onDragEnd: vi.fn(), onSelect: vi.fn() };

	it('draws a BT handle per point and an ET handle where there is an ET, the selected one red', () => {
		const points = [pt(600, 215), pt(0, 180, { target_env_temp: 200 })];
		const axes = { x: { min: 0, max: 600 }, y: { min: 100, max: 300 } };
		const g = handleGraphics(instance, points, 1, axes, pal, on) as {
			x: number;
			y: number;
			style: { fill: string };
			cursor: string;
		}[];
		expect(g).toHaveLength(3);
		// sorted by time: the point at 0 s (index 1, selected) first, with its ET handle
		expect(g[0]).toMatchObject({ x: 55, style: { fill: '#ef4444' }, cursor: 'move' });
		expect(g[1]).toMatchObject({ x: 55, style: { fill: '#ef4444' }, cursor: 'ns-resize' });
		expect(g[2]).toMatchObject({ x: 655, style: { fill: '#f59e0b' } });
	});

	it('reports a drag as a time and °C, rounded, and a click as the point', () => {
		const points = [pt(0, 180)];
		const axes = { x: { min: 0, max: 600 }, y: { min: 100, max: 300 } };
		const [h] = handleGraphics(instance, points, null, axes, pal, on) as unknown as {
			ondrag: (this: { x: number; y: number }) => void;
			ondragend: () => void;
			onclick: () => void;
		}[];
		(instance.convertFromPixel as ReturnType<typeof vi.fn>).mockReturnValueOnce([123.4, 150.26]);
		h.ondrag.call({ x: 1, y: 2 });
		expect(on.onDragBt).toHaveBeenLastCalledWith(0, [123, 150.3]);
		(instance.convertFromPixel as ReturnType<typeof vi.fn>).mockReturnValueOnce(null);
		h.ondrag.call({ x: 1, y: 2 });
		expect(on.onDragBt).toHaveBeenLastCalledWith(0, null);
		h.ondragend();
		expect(on.onDragEnd).toHaveBeenCalledWith(0);
		h.onclick();
		expect(on.onSelect).toHaveBeenCalledWith(0);
	});

	it('reports an ET handle’s drag as °C, and null when the pixel maps to nothing', () => {
		const points = [pt(0, 180, { target_env_temp: 200 })];
		const axes = { x: { min: 0, max: 600 }, y: { min: 100, max: 300 } };
		const [, et] = handleGraphics(instance, points, null, axes, pal, on) as unknown as {
			ondrag: (this: { x: number; y: number }) => void;
			onclick: () => void;
		}[];
		(instance.convertFromPixel as ReturnType<typeof vi.fn>).mockReturnValueOnce([10, 210.04]);
		et.ondrag.call({ x: 1, y: 2 });
		expect(on.onDragEt).toHaveBeenLastCalledWith(0, 210);
		(instance.convertFromPixel as ReturnType<typeof vi.fn>).mockReturnValueOnce(null);
		et.ondrag.call({ x: 1, y: 2 });
		expect(on.onDragEt).toHaveBeenLastCalledWith(0, null);
		et.onclick();
		expect(on.onSelect).toHaveBeenLastCalledWith(0);
	});
});
