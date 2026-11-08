import { describe, expect, it, vi } from 'vitest';

// uPlot listens for pixel-ratio changes through matchMedia when it loads; jsdom has none.
vi.hoisted(() => {
	vi.stubGlobal('matchMedia', () => ({
		matches: false,
		addEventListener() {},
		removeEventListener() {},
		addListener() {},
		removeListener() {}
	}));
});
import type uPlot from 'uplot';
import { chartPalette } from '#lib/constants/chartTheme.js';
import { BAND, CH0, MODEL_HI, MODEL_LO, SERIES, TOGGLEABLE, type SeriesKey } from './model.js';
import { scopeOptions, type ScopeOptionsInput } from './options.js';

const visible = Object.fromEntries(TOGGLEABLE.map((k) => [k, true])) as Record<SeriesKey, boolean>;
const hooks = { drawClear: vi.fn(), draw: vi.fn(), setCursor: vi.fn(), onZoomSelect: vi.fn() };
const input: ScopeOptionsInput = {
	width: 900,
	height: 400,
	pal: chartPalette(),
	channels: [],
	visible,
	channelShown: () => true,
	xRange: (_u, min, max) => [min, max],
	channelTop: () => null,
	hooks
};

describe('the scope options', () => {
	it('declares the series in the model’s layout and the bands between them', () => {
		const o = scopeOptions(input);
		expect(o.series).toHaveLength(CH0);
		expect(o.series[SERIES.bt].label).toBe('BT');
		expect(o.series[BAND].label).toBe('Δ band');
		expect(o.series[MODEL_LO].label).toBe('Model low');
		expect(o.series[MODEL_HI].label).toBe('Model high');
		expect(o.bands!.map((b) => b.series)).toEqual([
			[SERIES.bt, BAND],
			[BAND, SERIES.bt],
			[MODEL_HI, MODEL_LO]
		]);
		expect(Object.keys(o.scales!)).toEqual(['x', 'temp', 'ror', 'pct', 'tempProj', 'rorProj']);
		expect(o.axes).toHaveLength(4);
	});

	it('hides a series the toggles switched off, the band with the profile, the model band with the model', () => {
		const o = scopeOptions({
			...input,
			visible: { ...visible, et: false, profBt: false, model: false }
		});
		expect(o.series[SERIES.et].show).toBe(false);
		expect(o.series[SERIES.bt].show).toBe(true);
		expect(o.series[BAND].show).toBe(false);
		expect(o.series[MODEL_LO].show).toBe(false);
	});

	it('gives a channel in a unit of its own a scale and an axis, up to its top', () => {
		const o = scopeOptions({
			...input,
			channels: [
				{ key: 't3', label: 'Inlet', unit: 'C', max: null },
				{ key: 'pressure', label: 'Pressure', unit: 'bar', max: 4 }
			],
			channelShown: (k) => k !== 't3',
			channelTop: (i) => (i === 1 ? 3.2 : null)
		});
		expect(o.series).toHaveLength(CH0 + 2);
		expect(o.series[CH0]).toMatchObject({ label: 'Inlet', scale: 'temp', show: false });
		expect(o.series[CH0 + 1]).toMatchObject({
			label: 'Pressure',
			scale: 'ch:pressure',
			show: true
		});
		expect(o.axes).toHaveLength(5);
		const range = o.scales!['ch:pressure'].range as () => [number, number];
		expect(range()).toEqual([0, 3.2]);
	});

	it('keeps the RoR axis readable through the charge drop', () => {
		const o = scopeOptions(input);
		const ror = o.scales!.ror.range as (u: uPlot, min: number, max: number) => [number, number];
		const u = {} as uPlot;
		expect(ror(u, -200, 12)).toEqual([-30, 25]);
		expect(ror(u, 2, 18)).toEqual([-5, 25]);
		expect(ror(u, 0, 90)).toEqual([-5, 60]);
		const temp = o.scales!.temp.range as (u: uPlot, min: number, max: number) => [number, number];
		expect(temp(u, 21, 23)).toEqual([10, 60]);
		expect(temp(u, 95, 215)).toEqual([80, 230]);
	});

	it('asks the component for the x range and writes the time axis as m:ss', () => {
		const xRange = vi.fn((_u: uPlot, min: number, max: number): [number, number] => [
			min - 15,
			max
		]);
		const o = scopeOptions({ ...input, xRange });
		const range = o.scales!.x.range as (u: uPlot, min: number, max: number) => [number, number];
		const u = {} as uPlot;
		expect(range(u, 0, 600)).toEqual([-15, 600]);
		expect(xRange).toHaveBeenCalledWith(u, 0, 600);
		const values = o.axes![0].values as (u: uPlot, vals: number[]) => string[];
		expect(values(u, [0, 90, 600])).toEqual(['0:00', '1:30', '10:00']);
		const time = o.series[0].value as (u: uPlot, v: number | null) => string;
		expect(time(u, 125)).toBe('2:05');
		expect(time(u, null)).toBe('');
		const mirror = o.scales!.tempProj.range as (u: uPlot) => [number, number];
		expect(mirror({ scales: { temp: { min: 80, max: 230 } } } as unknown as uPlot)).toEqual([
			80, 230
		]);
	});

	it('drops the axis labels on a narrow chart', () => {
		const o = scopeOptions({ ...input, width: 400 });
		expect(o.axes![1]).toMatchObject({ labelSize: 0 });
		expect(o.axes![3].show).toBe(false);
	});

	it('wires the hooks the component draws and zooms with', () => {
		const o = scopeOptions(input);
		expect(o.hooks!.drawClear).toEqual([hooks.drawClear]);
		expect(o.hooks!.draw).toEqual([hooks.draw]);
		expect(o.hooks!.setCursor).toEqual([hooks.setCursor]);
		o.hooks!.setSelect![0]!({ select: { width: 12 } } as uPlot);
		expect(hooks.onZoomSelect).toHaveBeenCalledTimes(1);
		o.hooks!.setSelect![0]!({ select: { width: 0 } } as uPlot);
		expect(hooks.onZoomSelect).toHaveBeenCalledTimes(1);
	});
});
