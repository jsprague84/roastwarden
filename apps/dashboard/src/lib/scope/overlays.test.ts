import { describe, expect, it, vi } from 'vitest';
import type uPlot from 'uplot';
import { chartPalette } from '#lib/constants/chartTheme.js';
import { landmarkColors } from '#lib/constants/landmarks.js';
import {
	drawDevWindow,
	drawDropPrediction,
	drawEventFlags,
	drawNowMarker,
	drawProfileTicks
} from './overlays.js';
import { SERIES_COLORS } from './draw.js';
import { TOGGLEABLE, type SeriesKey } from './model.js';
import type { DevGuide, EventFlag, ProfileTick } from './scopeData.js';

const pal = chartPalette();
const visible = Object.fromEntries(TOGGLEABLE.map((k) => [k, true])) as Record<SeriesKey, boolean>;

/** A chart 600 px wide from `min` to `max` seconds, its canvas recording every call. */
function fakeChart(min: number, max: number) {
	const bbox = { left: 50, top: 20, width: 600, height: 300 };
	const ctx = {
		font: '',
		textAlign: '',
		textBaseline: '',
		fillStyle: '',
		strokeStyle: '',
		lineWidth: 0,
		globalAlpha: 1,
		save: vi.fn(),
		restore: vi.fn(),
		beginPath: vi.fn(),
		closePath: vi.fn(),
		rect: vi.fn(),
		clip: vi.fn(),
		moveTo: vi.fn(),
		lineTo: vi.fn(),
		fill: vi.fn(),
		stroke: vi.fn(),
		fillRect: vi.fn(),
		fillText: vi.fn(),
		setLineDash: vi.fn(),
		measureText: vi.fn((s: string) => ({ width: 6 * s.length }))
	};
	const u = {
		ctx,
		bbox,
		scales: { x: { min, max } },
		valToPos: (v: number) => bbox.left + ((v - min) / (max - min)) * bbox.width
	} as unknown as uPlot;
	return { u, ctx, bbox };
}

const tempText = (c: number) => `${Math.round(c)}`;

describe('event flags', () => {
	const flags: EventFlag[] = [
		{ t: 10, kind: 'heater', value: 80 },
		{ t: 11, kind: 'fan', value: 60 }, // another kind: not decluttered against the heater
		{ t: 12, kind: 'heater', value: 85 }, // 2 s = 2 px after the first heater flag: left out
		{ t: 300, kind: 'sp', value: 215 },
		{ t: 900, kind: 'fan', value: 50 } // past the right edge
	];

	it('draws a tick per visible flag inside the plot, decluttered per kind', () => {
		const { u, ctx } = fakeChart(0, 600);
		drawEventFlags(u, flags, visible, pal, tempText);
		expect(ctx.fill).toHaveBeenCalledTimes(3);
		// the fan flag's value would overlap the heater's, one pixel on: the tick is drawn, its value is not
		expect(ctx.fillText.mock.calls.map((c) => c[0])).toEqual(['80', '215']);
		expect(ctx.textAlign).toBe('center');
	});

	it('leaves out a kind whose series is hidden, and the values when zoomed out', () => {
		const { u, ctx } = fakeChart(0, 600);
		drawEventFlags(u, flags, { ...visible, heater: false }, pal, tempText);
		expect(ctx.fill).toHaveBeenCalledTimes(2);
		const wide = fakeChart(0, 2000);
		drawEventFlags(wide.u, flags, visible, pal, tempText);
		expect(wide.ctx.fill).toHaveBeenCalledTimes(4); // the far fan flag is in view now
		expect(wide.ctx.fillText).not.toHaveBeenCalled();
	});

	it('writes a value only where it does not overlap the last one', () => {
		const { u, ctx } = fakeChart(0, 600);
		drawEventFlags(
			u,
			[
				{ t: 100, kind: 'heater', value: 80 },
				{ t: 101, kind: 'fan', value: 60 }, // 1 px on: its text would overlap
				{ t: 200, kind: 'sp', value: 200 }
			],
			visible,
			pal,
			tempText
		);
		expect(ctx.fill).toHaveBeenCalledTimes(3);
		expect(ctx.fillText.mock.calls.map((c) => c[0])).toEqual(['80', '200']);
	});
});

describe('profile ticks', () => {
	const ticks: ProfileTick[] = [
		{
			t: 100,
			action: 'fan',
			event: {
				trigger_kind: 'time',
				at_seconds: 100,
				at_bt: null,
				action: 'fan',
				value: 50,
				label: null
			}
		},
		{
			t: 400,
			action: 'landmark',
			event: {
				trigger_kind: 'bt',
				at_seconds: null,
				at_bt: 196,
				action: 'landmark',
				value: null,
				label: 'first_crack_start'
			}
		}
	];

	it('draws past ticks hollow and upcoming ones solid, in the landmark’s colour, with their text', () => {
		const { u, ctx } = fakeChart(0, 600);
		const describe = vi.fn((e: ProfileTick['event']) => e.action);
		drawProfileTicks(u, ticks, 300, pal, describe);
		expect(ctx.stroke).toHaveBeenCalledTimes(1); // the fan change already fired
		expect(ctx.fill).toHaveBeenCalledTimes(1); // first crack is still to come
		expect(ctx.fillStyle).toBe(landmarkColors['first_crack_start']);
		expect(ctx.fillText.mock.calls.map((c) => c[0])).toEqual(['fan', 'landmark']);
		expect(describe).toHaveBeenCalledTimes(2);
		expect(ctx.globalAlpha).toBe(1);
	});

	it('writes no text when ticks crowd or the view is wide', () => {
		const close = fakeChart(0, 600);
		drawProfileTicks(close.u, [ticks[0], { ...ticks[1], t: 110 }], 300, pal, (e) => e.action);
		expect(close.ctx.fillText).toHaveBeenCalledTimes(1);
		const wide = fakeChart(0, 3600);
		drawProfileTicks(wide.u, ticks, 300, pal, (e) => e.action);
		expect(wide.ctx.fillText).not.toHaveBeenCalled();
		expect(wide.ctx.stroke).toHaveBeenCalledTimes(1);
	});
});

describe('the now marker', () => {
	it('shades the horizon beyond now and draws a dashed line there', () => {
		const { u, ctx, bbox } = fakeChart(0, 600);
		drawNowMarker(u, 480, pal, false);
		expect(ctx.fillRect).toHaveBeenCalledWith(530, bbox.top, 120, bbox.height);
		expect(ctx.fillStyle).toBe('rgba(255,255,255,0.03)');
		expect(ctx.stroke).toHaveBeenCalledTimes(1);
		expect(ctx.setLineDash).toHaveBeenLastCalledWith([]);
		const light = fakeChart(0, 600);
		drawNowMarker(light.u, 480, pal, true);
		expect(light.ctx.fillStyle).toBe('rgba(0,0,0,0.03)');
	});

	it('draws nothing when now is out of view', () => {
		const { u, ctx } = fakeChart(0, 300);
		drawNowMarker(u, 480, pal, false);
		expect(ctx.fillRect).not.toHaveBeenCalled();
		expect(ctx.stroke).not.toHaveBeenCalled();
	});
});

describe('the predicted DROP and the development window', () => {
	const dev: DevGuide = {
		devSec: 60,
		dtr: 12,
		dT: 8,
		targetDtr: 20,
		dropAtSec: 540,
		secsToDrop: 60,
		btAtDrop: 212,
		secsToDropBt: null,
		targetDropBt: null
	};

	it('marks the DROP ahead with its target, not once it is past', () => {
		const { u, ctx } = fakeChart(0, 600);
		drawDropPrediction(u, dev);
		expect(ctx.stroke).toHaveBeenCalledTimes(1);
		expect(ctx.fillText).toHaveBeenCalledWith('DROP 20%', 590, 22);
		expect(ctx.globalAlpha).toBe(1);
		const past = fakeChart(0, 600);
		drawDropPrediction(past.u, { ...dev, secsToDrop: -5 });
		expect(past.ctx.stroke).not.toHaveBeenCalled();
		const out = fakeChart(0, 300);
		drawDropPrediction(out.u, dev);
		expect(out.ctx.stroke).not.toHaveBeenCalled();
	});

	it('brackets the expected development window inside the plot', () => {
		const { u, ctx, bbox } = fakeChart(0, 600);
		drawDevWindow(u, { from: 480, to: 660, lo: 60, hi: 120 });
		expect(ctx.moveTo).toHaveBeenCalledTimes(1);
		expect(ctx.lineTo).toHaveBeenCalledTimes(3);
		// clipped to the right edge
		expect(ctx.lineTo.mock.calls[1][0]).toBe(bbox.left + bbox.width);
		expect(ctx.stroke).toHaveBeenCalledTimes(1);
		expect(ctx.globalAlpha).toBe(1);
		const out = fakeChart(0, 300);
		drawDevWindow(out.u, { from: 480, to: 660, lo: 60, hi: 120 });
		expect(out.ctx.stroke).not.toHaveBeenCalled();
	});
});

describe('the colours', () => {
	it('flags take their series’ colours', () => {
		const { u, ctx } = fakeChart(0, 600);
		drawEventFlags(u, [{ t: 300, kind: 'sp', value: 200 }], visible, pal, tempText);
		expect(ctx.fillStyle).toBe(SERIES_COLORS.sp);
	});
});
