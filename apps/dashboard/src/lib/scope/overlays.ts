/**
 * The live scope's overlays, drawn on uPlot's canvas each tick over the curves: event flags
 * along the bottom edge, the profile's event ticks along the top, the "now" marker with the
 * projection's shading, the predicted DROP for the target development, and the expected
 * development window. draw.ts holds what the roast demo shares (phase bands, landmarks).
 */
import type uPlot from 'uplot';
import type { ChartPalette } from '#lib/constants/chartPalettes.js';
import { landmarkColors } from '#lib/constants/landmarks.js';
import { SERIES_COLORS } from './draw.js';
import type { DevWindow, SeriesKey } from './model.js';
import type { DevGuide, EventFlag, ProfileEventLike, ProfileTick } from './scopeData.js';

/** Event flags: small ticks along the bottom edge, their values in one row above when zoomed in. */
export function drawEventFlags(
	u: uPlot,
	flags: EventFlag[],
	visible: Record<SeriesKey, boolean>,
	pal: ChartPalette,
	tempText: (c: number) => string
): void {
	const { ctx, bbox } = u;
	const dpr = devicePixelRatio;
	const flagColor: Record<EventFlag['kind'], string> = {
		heater: SERIES_COLORS.heater,
		fan: SERIES_COLORS.fan,
		sp: SERIES_COLORS.sp
	};
	ctx.textBaseline = 'bottom';
	const lastX: Record<EventFlag['kind'], number> = {
		heater: -Infinity,
		fan: -Infinity,
		sp: -Infinity
	};
	const minGap = 14 * dpr;
	const withValues = u.scales.x.max! - u.scales.x.min! < 1500;
	ctx.font = `${9 * dpr}px ${pal.font}`;
	// Values share one row above the ticks: a value that would overlap the last is left out.
	let valueEnd = -Infinity;
	for (const f of flags) {
		if (f.kind === 'heater' && !visible.heater) continue;
		if (f.kind === 'fan' && !visible.fan) continue;
		if (f.kind === 'sp' && !visible.sp) continue;
		const x = u.valToPos(f.t, 'x', true);
		if (x < bbox.left || x > bbox.left + bbox.width) continue;
		if (x - lastX[f.kind] < minGap) continue; // declutter when zoomed out
		lastX[f.kind] = x;
		const y = bbox.top + bbox.height;
		ctx.fillStyle = flagColor[f.kind];
		ctx.beginPath();
		ctx.moveTo(x, y - 8 * dpr);
		ctx.lineTo(x - 4 * dpr, y);
		ctx.lineTo(x + 4 * dpr, y);
		ctx.closePath();
		ctx.fill();
		if (withValues) {
			const v = f.kind === 'sp' ? tempText(f.value) : String(Math.round(f.value));
			const w = ctx.measureText(v).width;
			if (x - w / 2 >= valueEnd + 4 * dpr) {
				ctx.textAlign = 'center';
				ctx.fillText(v, x, y - 9 * dpr);
				valueEnd = x + w / 2;
			}
		}
	}
}

/** Profile events: hollow ticks along the top edge, upcoming ones solid, with their text when
 * there is room. */
export function drawProfileTicks(
	u: uPlot,
	ticks: ProfileTick[],
	nowSec: number,
	pal: ChartPalette,
	describe: (e: ProfileEventLike) => string
): void {
	const { ctx, bbox } = u;
	const dpr = devicePixelRatio;
	ctx.font = `${9 * dpr}px ${pal.font}`;
	ctx.textAlign = 'left';
	ctx.textBaseline = 'top';
	let lastTickX = -Infinity;
	const tickColor: Record<string, string> = {
		heater: SERIES_COLORS.heater,
		fan: SERIES_COLORS.fan,
		setpoint: SERIES_COLORS.sp
	};
	for (const tk of ticks) {
		const x = u.valToPos(tk.t, 'x', true);
		if (x < bbox.left || x > bbox.left + bbox.width) continue;
		const upcoming = tk.t >= nowSec;
		const color =
			tk.action === 'landmark'
				? (landmarkColors[tk.event.label ?? ''] ?? pal.mutedText)
				: (tickColor[tk.action] ?? pal.mutedText);
		const y = bbox.top + 18 * dpr;
		ctx.globalAlpha = upcoming ? 1 : 0.45;
		ctx.strokeStyle = color;
		ctx.fillStyle = color;
		ctx.lineWidth = 1 * dpr;
		ctx.beginPath();
		ctx.moveTo(x, y + 7 * dpr);
		ctx.lineTo(x - 4 * dpr, y);
		ctx.lineTo(x + 4 * dpr, y);
		ctx.closePath();
		if (upcoming) ctx.fill();
		else ctx.stroke();
		if (x - lastTickX > 70 * dpr && u.scales.x.max! - u.scales.x.min! < 1800) {
			ctx.fillText(describe(tk.event), x + 5 * dpr, y - 1 * dpr);
			lastTickX = x;
		}
	}
	ctx.globalAlpha = 1;
}

/** The "now" marker: a dashed line, the projection's horizon shaded beyond it. */
export function drawNowMarker(u: uPlot, nowSec: number, pal: ChartPalette, light: boolean): void {
	const { ctx, bbox } = u;
	const dpr = devicePixelRatio;
	const xNow = u.valToPos(nowSec, 'x', true);
	if (xNow >= bbox.left && xNow <= bbox.left + bbox.width) {
		ctx.fillStyle = light ? 'rgba(0,0,0,0.03)' : 'rgba(255,255,255,0.03)';
		ctx.fillRect(xNow, bbox.top, bbox.left + bbox.width - xNow, bbox.height);
		ctx.strokeStyle = pal.mutedText;
		ctx.lineWidth = 1 * dpr;
		ctx.setLineDash([2 * dpr, 3 * dpr]);
		ctx.beginPath();
		ctx.moveTo(xNow, bbox.top);
		ctx.lineTo(xNow, bbox.top + bbox.height);
		ctx.stroke();
		ctx.setLineDash([]);
	}
}

/** The predicted DROP for the target development ratio: a red dashed line, labelled. */
export function drawDropPrediction(u: uPlot, dev: DevGuide): void {
	if (!(dev.secsToDrop > 0)) return;
	const { ctx, bbox } = u;
	const dpr = devicePixelRatio;
	const xd = u.valToPos(dev.dropAtSec, 'x', true);
	if (xd >= bbox.left && xd <= bbox.left + bbox.width) {
		ctx.strokeStyle = '#ef4444';
		ctx.globalAlpha = 0.7;
		ctx.lineWidth = 1.5 * dpr;
		ctx.setLineDash([3 * dpr, 3 * dpr]);
		ctx.beginPath();
		ctx.moveTo(xd, bbox.top);
		ctx.lineTo(xd, bbox.top + bbox.height);
		ctx.stroke();
		ctx.setLineDash([]);
		ctx.globalAlpha = 1;
		ctx.fillStyle = '#ef4444';
		ctx.textAlign = 'center';
		ctx.textBaseline = 'top';
		ctx.fillText(`DROP ${dev.targetDtr.toFixed(0)}%`, xd, bbox.top + 2 * dpr);
	}
}

/** The expected development window (roast guidance): a bracket under the top edge. */
export function drawDevWindow(u: uPlot, w: DevWindow): void {
	const { ctx, bbox } = u;
	const dpr = devicePixelRatio;
	const x0 = Math.max(bbox.left, u.valToPos(w.from, 'x', true));
	const x1 = Math.min(bbox.left + bbox.width, u.valToPos(w.to, 'x', true));
	if (x1 > x0) {
		const y = bbox.top + 14 * dpr;
		ctx.strokeStyle = '#ef4444';
		ctx.globalAlpha = 0.55;
		ctx.lineWidth = 2 * dpr;
		ctx.beginPath();
		ctx.moveTo(x0, y - 3 * dpr);
		ctx.lineTo(x0, y);
		ctx.lineTo(x1, y);
		ctx.lineTo(x1, y - 3 * dpr);
		ctx.stroke();
		ctx.globalAlpha = 1;
	}
}
