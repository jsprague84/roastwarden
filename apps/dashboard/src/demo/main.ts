/**
 * roastwarden.com's roast demo: a recorded roast played on
 * the dashboard's own scope drawing (uPlot, the series colours, phase bands and landmarks of
 * lib/scope), at 20× speed, looping. It pauses when off screen; with reduced motion it shows the
 * finished roast. Hovering reads a moment. Built by vite.demo.config.ts for the site; the home
 * page holds a `[data-roast-demo]` figure for it.
 */
import uPlot, { type AlignedData, type Options } from 'uplot';
import 'uplot/dist/uPlot.min.css';
import recorded from './roast.json';
import { paletteFor, type ChartPalette } from '#lib/constants/chartPalettes.js';
import { axisLayout, TIME_INCRS } from '#lib/scope/axes.js';
import {
	alignToBase,
	fmtClock,
	phaseBands,
	phasePercents,
	type Landmark,
	type Num,
	type PhaseBand
} from '#lib/scope/scopeData.js';
import {
	SERIES_COLORS,
	SERIES_LABELS,
	drawLandmarks,
	drawPhaseBands,
	type ScopeMark,
	type ScopeSeriesKey
} from '#lib/scope/draw.js';
import { HEATER_SMOOTH_SEC, smoothByTime } from '#lib/scope/smooth.js';
import { indexAt, rorRange, tempRange } from './frame.js';

export interface DemoRoast {
	title: string;
	/** Seconds from CHARGE */
	t: number[];
	bt: Num[];
	et: Num[];
	ror: Num[];
	heater: Num[];
	fan: Num[];
	events: { type: string; t: number; temp: number | null }[];
	profile?: { t: number[]; bt: Num[] };
}

const SPEED = 20;
const HOLD_MS = 6000;
/** The first play opens this far into the roast, s: charge and turning point already drawn, not an
 * empty chart at first sight (the home page's hero). Later loops play from the start. */
const FIRST_FROM_S = 90;
const FONT = "'Atkinson Hyperlegible Next', ui-sans-serif, system-ui, sans-serif";
const UNIT_KEY = 'roastwarden.demo.unit';
/** The series drawn, in uPlot's order after x. */
const SHOWN: ScopeSeriesKey[] = ['bt', 'et', 'rorBt', 'heater', 'fan', 'profBt'];

type Unit = 'C' | 'F';

function prefersLight(): boolean {
	const t = document.documentElement.dataset.theme;
	return t ? t === 'light' : !matchMedia('(prefers-color-scheme: dark)').matches;
}

function savedUnit(): Unit {
	try {
		const u = localStorage.getItem(UNIT_KEY);
		if (u === 'C' || u === 'F') return u;
	} catch {
		/* private mode */
	}
	return navigator.language === 'en-US' ? 'F' : 'C';
}

function el<K extends keyof HTMLElementTagNameMap>(
	tag: K,
	cls?: string,
	text?: string
): HTMLElementTagNameMap[K] {
	const e = document.createElement(tag);
	if (cls) e.className = cls;
	if (text != null) e.textContent = text;
	return e;
}

function mount(fig: HTMLElement, r: DemoRoast) {
	const n = r.t.length;
	const t0 = r.t[0];
	const tEnd = r.t[n - 1];
	// The heater as the live scope draws it by default: a 15 s average (the PID's output swings).
	const heater = smoothByTime(r.t, r.heater, HEATER_SMOOTH_SEC);
	const profileBt = r.profile ? alignToBase(r.t, r.profile.t, r.profile.bt) : r.t.map(() => null);
	const landmarks: Landmark[] = r.events.map((e) => ({
		type: e.type,
		elapsed_seconds: e.t,
		temperature: e.temp ?? undefined
	}));
	const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

	let unit = savedUnit();
	let simT = reduced ? tEnd : Math.min(tEnd, t0 + FIRST_FROM_S);
	let k = reduced ? n - 1 : indexAt(r.t, simT);
	let playing = !reduced;
	let onScreen = true;
	let raf = 0;
	let hold = 0;
	let last = 0;
	let hover: number | null = null;
	let pal: ChartPalette = { ...paletteFor(prefersLight()), font: FONT };
	let bands: PhaseBand[] = [];
	let marks: ScopeMark[] = [];

	const T = (c: number) => (unit === 'F' ? (c * 9) / 5 + 32 : c);
	const D = (c: number) => (unit === 'F' ? (c * 9) / 5 : c);
	const tu = () => (unit === 'F' ? '°F' : '°C');
	const fmtT = (c: Num) => (c == null ? '–' : T(c).toFixed(1));
	// Rounded, so an average of zeros never reads "-0 %"
	const pct = (v: Num) => (v == null ? '–' : `${Math.round(v) || 0} %`);

	// ---- the figure's parts ----
	fig.replaceChildren(...[...fig.children].filter((c) => c.tagName === 'FIGCAPTION'));
	const head = el('div', 'demo-head');
	const read = el('dl', 'demo-read');
	const controls = el('div', 'demo-controls');
	const units = el('div', 'demo-units');
	units.setAttribute('role', 'group');
	units.setAttribute('aria-label', 'Temperature unit');
	const unitButtons = (['C', 'F'] as Unit[]).map((u) => {
		const b = el('button', undefined, `°${u}`);
		b.type = 'button';
		b.addEventListener('click', () => setUnit(u));
		units.append(b);
		return b;
	});
	const play = el('button', 'demo-play');
	play.type = 'button';
	controls.append(units, play);
	head.append(read, controls);
	const plot = el('div', 'demo-plot');
	plot.setAttribute('aria-hidden', 'true');
	const legend = el('ul', 'demo-legend');
	for (const key of SHOWN) {
		const li = el('li');
		const sw = el('span', key === 'profBt' ? 'swatch dashed' : 'swatch');
		sw.style.setProperty('--c', SERIES_COLORS[key]);
		li.append(sw, SERIES_LABELS[key]);
		legend.append(li);
	}
	fig.prepend(head, plot, legend);

	const cells = new Map<string, HTMLElement>();
	for (const [key, label, color] of [
		['time', 'Time', ''],
		['phase', 'Phase', ''],
		['bt', 'BT', SERIES_COLORS.bt],
		['ror', 'RoR', SERIES_COLORS.rorBt],
		['heater', 'Heater', SERIES_COLORS.heater],
		['fan', 'Fan', SERIES_COLORS.fan]
	] as const) {
		const div = el('div');
		const dt = el('dt', undefined, label);
		// The series colour as a swatch: as text it is too faint on the light page (contrast).
		if (color) dt.style.setProperty('--swatch', color);
		if (color) dt.classList.add('swatch');
		const dd = el('dd');
		div.append(dt, dd);
		read.append(div);
		cells.set(key, dd);
	}

	// ---- data and drawing ----
	function frameData(): AlignedData {
		const cut = (a: Num[], f: (v: number) => number) =>
			a.map((v, i) => (i <= k && v != null ? f(v) : null));
		const same = (v: number) => v;
		return [
			r.t,
			cut(r.bt, T),
			cut(r.et, T),
			cut(r.ror, D),
			cut(heater, same),
			cut(r.fan, same),
			profileBt.map((v) => (v == null ? null : T(v)))
		];
	}

	function frameMarks() {
		const now = r.t[k];
		const seen = landmarks.filter((l) => l.elapsed_seconds <= now);
		bands = phaseBands(seen, null, 0, now);
		marks = seen.map((l) => ({ t: l.elapsed_seconds, type: l.type, temp: l.temperature ?? null }));
	}

	function options(width: number, height: number): Options {
		const lay = axisLayout(width);
		const font = `${lay.font}px ${pal.font}`;
		const grid = { stroke: pal.line, width: 1 };
		const ticks = { stroke: pal.line, width: 1 };
		const label = (l: string) =>
			lay.labels ? { label: l, labelFont: font, labelSize: 14 } : { labelSize: 0 };
		const temps = tempRange([...r.bt, ...r.et, ...profileBt].map((v) => (v == null ? null : T(v))));
		const rors = rorRange(
			r.ror.map((v) => (v == null ? null : D(v))),
			D
		);
		const line = (key: ScopeSeriesKey, scale: string, extra: object = {}) => ({
			label: SERIES_LABELS[key],
			scale,
			stroke: SERIES_COLORS[key],
			width: 2,
			points: { show: false },
			spanGaps: false,
			...extra
		});
		const stepped = uPlot.paths.stepped!({ align: 1 });
		return {
			width,
			height,
			padding: [40, 8, 0, 0],
			cursor: { drag: { x: false, y: false }, points: { size: 7 }, focus: { prox: 24 } },
			legend: { show: false },
			scales: {
				x: { time: false, range: () => [t0, tEnd] },
				temp: { range: () => temps },
				ror: { range: () => rors },
				pct: { range: [0, 100] }
			},
			axes: [
				{
					stroke: pal.axisText,
					grid,
					ticks,
					font,
					values: (u, vals) => vals.map(fmtClock),
					space: lay.xSpace,
					incrs: TIME_INCRS
				},
				{ scale: 'temp', ...label(tu()), stroke: pal.axisText, grid, ticks, font, size: lay.temp },
				{
					scale: 'ror',
					side: 1,
					...label(`${tu()}/min`),
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
				}
			],
			series: [
				{},
				line('bt', 'temp', { width: 2.5 }),
				line('et', 'temp'),
				line('rorBt', 'ror'),
				line('heater', 'pct', { width: 1.5, paths: stepped }),
				line('fan', 'pct', { width: 1.5, paths: stepped }),
				line('profBt', 'temp', { width: 1.5, dash: [4, 4] })
			],
			hooks: {
				drawClear: [(u) => drawPhaseBands(u, bands, phasePercents(bands), pal)],
				draw: [
					(u) => {
						const { ctx, bbox } = u;
						ctx.save();
						ctx.beginPath();
						ctx.rect(
							bbox.left,
							bbox.top - 38 * devicePixelRatio,
							bbox.width,
							bbox.height + 38 * devicePixelRatio
						);
						ctx.clip();
						drawLandmarks(u, marks, pal, (c) => T(c).toFixed(0));
						ctx.restore();
					}
				],
				setCursor: [
					(u) => {
						const i = u.cursor.idx;
						hover = i != null && i <= k ? i : null;
						readout();
					}
				]
			}
		};
	}

	function readout() {
		const i = hover ?? k;
		const sec = r.t[i];
		const phase = bands.find((b) => sec >= b.from && sec <= b.to);
		const set = (key: string, v: string) => (cells.get(key)!.textContent = v);
		set('time', sec < 0 ? `-${fmtClock(-sec)}` : fmtClock(sec));
		set(
			'phase',
			phase ? (phase.kind === 'cooling' ? 'Cooling' : phase.label) : sec < 0 ? 'Charge' : '–'
		);
		set('bt', `${fmtT(r.bt[i])} ${tu()}`);
		set('ror', r.ror[i] == null ? '–' : `${D(r.ror[i]!).toFixed(1)} ${tu()}/min`);
		set('heater', pct(heater[i]));
		set('fan', pct(r.fan[i]));
	}

	function controlsState() {
		play.textContent = playing ? 'Pause' : k >= n - 1 ? 'Replay' : 'Play';
		play.setAttribute(
			'aria-label',
			playing ? 'Pause the roast' : k >= n - 1 ? 'Play the roast again' : 'Play the roast'
		);
		unitButtons.forEach((b, j) =>
			b.setAttribute('aria-pressed', String((j === 0 ? 'C' : 'F') === unit))
		);
	}

	const size = () => {
		const w = Math.max(280, Math.floor(plot.clientWidth));
		return { width: w, height: w < 560 ? 260 : 340 };
	};
	let { width, height } = size();
	let compact = axisLayout(width).compact;
	frameMarks();
	let u = new uPlot(options(width, height), frameData(), plot);
	readout();
	controlsState();

	function rebuild() {
		({ width, height } = size());
		compact = axisLayout(width).compact;
		u.destroy();
		u = new uPlot(options(width, height), frameData(), plot);
		readout();
	}

	function render() {
		frameMarks();
		u.setData(frameData());
		if (hover == null) readout();
	}

	// ---- playback ----
	function tick(now: number) {
		raf = 0;
		if (!playing || !onScreen || document.hidden) return;
		simT += ((now - last) / 1000) * SPEED;
		last = now;
		if (simT >= tEnd) {
			simT = tEnd;
			k = n - 1;
			render();
			playing = false;
			controlsState();
			hold = window.setTimeout(() => restart(), HOLD_MS);
			return;
		}
		const next = indexAt(r.t, simT);
		if (next !== k) {
			k = next;
			render();
		}
		raf = requestAnimationFrame(tick);
	}

	function resume() {
		if (raf || !playing || !onScreen || document.hidden) return;
		last = performance.now();
		raf = requestAnimationFrame(tick);
	}

	function restart() {
		clearTimeout(hold);
		simT = t0;
		k = 0;
		playing = true;
		render();
		controlsState();
		resume();
	}

	play.addEventListener('click', () => {
		if (playing) {
			playing = false;
			cancelAnimationFrame(raf);
			raf = 0;
		} else if (k >= n - 1) {
			restart();
			return;
		} else {
			playing = true;
			resume();
		}
		clearTimeout(hold);
		controlsState();
	});

	function setUnit(next: Unit) {
		if (next === unit) return;
		unit = next;
		try {
			localStorage.setItem(UNIT_KEY, unit);
		} catch {
			/* private mode */
		}
		rebuild();
		controlsState();
	}

	new IntersectionObserver((entries) => {
		onScreen = entries.some((e) => e.isIntersecting);
		resume();
	}).observe(fig);
	document.addEventListener('visibilitychange', resume);

	new ResizeObserver(() => {
		const s = size();
		if (s.width === width && s.height === height) return;
		if (axisLayout(s.width).compact !== compact) return rebuild();
		({ width, height } = s);
		u.setSize(s);
	}).observe(plot);

	const retheme = () => {
		pal = { ...paletteFor(prefersLight()), font: FONT };
		rebuild();
	};
	new MutationObserver(retheme).observe(document.documentElement, {
		attributes: true,
		attributeFilter: ['data-theme']
	});
	matchMedia('(prefers-color-scheme: dark)').addEventListener('change', retheme);

	resume();
}

function start() {
	const fig = document.querySelector<HTMLElement>('[data-roast-demo]');
	if (!fig) return;
	// The font first: the chart's canvas text is drawn once per frame, not reflowed.
	const ready = document.fonts?.ready ?? Promise.resolve();
	void ready.then(() => mount(fig, recorded as DemoRoast));
}

if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
else start();
