<script lang="ts">
	/**
	 * Live roast scope on uPlot.
	 *
	 * Why uPlot and not ECharts here: the scope redraws every second with up to
	 * two hours of 1 Hz data, plus overlays. uPlot is a canvas renderer built for
	 * exactly that (incremental setData, sub-millisecond paints) and it exposes
	 * draw hooks, so phase bands, landmarks, event flags and projections are
	 * painted directly instead of being rebuilt as chart options each tick.
	 */
	import uPlot from 'uplot';
	import 'uplot/dist/uPlot.min.css';
	import { onMount, untrack } from 'svelte';
	import type { ProfileEventInput } from '#lib/api/profiles.js';
	import type { RoastSession } from '#lib/api/sessions.js';
	import { settings } from '#lib/api/settings.js';
	import ScopeHeader from '#lib/components/roast/scope/ScopeHeader.svelte';
	import SeriesToggles from '#lib/components/roast/scope/SeriesToggles.svelte';
	import { chartPalette } from '#lib/constants/chartTheme.js';
	import { describeEvent } from '#lib/profile/events.js';
	import { axisLayout } from '#lib/scope/axes.js';
	import { drawLandmarks, drawPhaseBands } from '#lib/scope/draw.js';
	import {
		BAND,
		CH0,
		MODEL_HI,
		MODEL_LO,
		SERIES,
		cursorAt,
		scopeModel,
		scopeReadout,
		type ScopeCursor,
		type SeriesKey
	} from '#lib/scope/model.js';
	import { scopeOptions } from '#lib/scope/options.js';
	import {
		drawDevWindow,
		drawDropPrediction,
		drawEventFlags,
		drawNowMarker,
		drawProfileTicks
	} from '#lib/scope/overlays.js';
	import type { Landmark, ScopeView } from '#lib/scope/scopeData.js';
	import { panWindow, pinchWindow, zoomWindow } from '#lib/scope/zoom.js';
	import { chartPrefs } from '#lib/stores/chartPrefs.svelte.js';
	import { kept, keep } from '#lib/stores/kept.js';
	import { predictionState, watchPrediction } from '#lib/stores/prediction.svelte.js';
	import { profileState } from '#lib/stores/profile.svelte.js';
	import { roast } from '#lib/stores/roast.svelte.js';
	import { replayState, telemetryHistory } from '#lib/stores/telemetry.js';
	import { isLight } from '#lib/stores/theme.svelte.js';
	import { fmtTemp, unitState } from '#lib/stores/units.svelte.js';

	let {
		landmarks = [],
		activeSession = null,
		liveReadout = true
	}: {
		landmarks?: Landmark[];
		activeSession?: RoastSession | null;
		/** Show BT/ET/RoR/Δ in the header when not hovering (off when big readouts sit above the scope). */
		liveReadout?: boolean;
	} = $props();

	const pal = $derived(chartPalette());

	const VIS_KEY = 'roastwarden.scope.series';
	const DEFAULT_VIS: Record<SeriesKey, boolean> = {
		bt: true,
		et: true,
		sp: true,
		rorBt: true,
		rorEt: false,
		heater: true,
		fan: true,
		profBt: true,
		profEt: false,
		profRor: true,
		projBt: true,
		projRor: false,
		model: false
	};
	function loadVis(): Record<SeriesKey, boolean> {
		try {
			const raw = localStorage.getItem(VIS_KEY);
			if (raw)
				return { ...DEFAULT_VIS, ...(JSON.parse(raw) as Partial<Record<SeriesKey, boolean>>) };
		} catch {
			/* ignore */
		}
		return { ...DEFAULT_VIS };
	}
	let visible = $state<Record<SeriesKey, boolean>>(loadVis());
	function toggle(k: SeriesKey) {
		visible[k] = !visible[k];
		try {
			localStorage.setItem(VIS_KEY, JSON.stringify(visible));
		} catch {
			/* ignore */
		}
		chart?.setSeries(SERIES[k], { show: visible[k] });
		if (k === 'profBt') chart?.setSeries(BAND, { show: visible.profBt });
		if (k === 'model') {
			chart?.setSeries(MODEL_LO, { show: visible.model });
			chart?.setSeries(MODEL_HI, { show: visible.model });
		}
	}

	// ---- declared channels (docs/channels.md): series after the fixed ones ----
	const channels = $derived(
		roast.setup && roast.setup.device_id === roast.deviceId ? (roast.setup.channels ?? []) : []
	);
	/** Changes when the channel set does: the chart is rebuilt (series and scales differ). */
	const channelSig = $derived(
		channels.map((c) => `${c.key}:${c.unit}:${c.max ?? ''}:${c.label}`).join(',')
	);
	const CH_VIS_KEY = 'roastwarden.scope.channels';
	function loadChVis(): Record<string, boolean> {
		try {
			const raw = localStorage.getItem(CH_VIS_KEY);
			if (raw) return JSON.parse(raw) as Record<string, boolean>;
		} catch {
			/* ignore */
		}
		return {};
	}
	let chVisible = $state<Record<string, boolean>>(loadChVis());
	const chShown = (key: string) => chVisible[key] !== false;
	function toggleChannel(i: number, key: string) {
		chVisible[key] = !chShown(key);
		try {
			localStorage.setItem(CH_VIS_KEY, JSON.stringify(chVisible));
		} catch {
			/* ignore */
		}
		chart?.setSeries(CH0 + i, { show: chShown(key) });
	}

	// The server's prediction, polled only while its series is on and a roast runs.
	$effect(() => {
		const id = activeSession?.device_id;
		if (!visible.model || !id) return;
		return watchPrediction(id);
	});

	// Kept for this tab, so a trip to another page does not reset it
	let viewChoice = $state<ScopeView | null>(kept<ScopeView | null>('scope.view', null));
	/** No roast yet, a profile picked: the scope previews it from "now". */
	const previewing = $derived(!activeSession && !!profileState.activeProfile);
	const hasFrame = $derived(!!activeSession || previewing);
	const view = $derived<ScopeView>(
		viewChoice === 'roast' && !hasFrame ? 'live10' : (viewChoice ?? (hasFrame ? 'roast' : 'live10'))
	);
	function chooseView(v: ScopeView) {
		viewChoice = v;
		keep('scope.view', v);
		resetZoom();
	}

	/** Development target when the profile has none (setting `target_dtr_pct`). */
	let targetDtrSetting = $state(20);
	$effect(() => {
		settings
			.get()
			.then((s) => {
				const v = parseFloat(s['target_dtr_pct'] ?? '');
				if (Number.isFinite(v) && v > 0 && v < 60) targetDtrSetting = v;
			})
			.catch(() => {});
	});

	// ---- derived scope model (°C → display units happens here, once per tick) ----
	const sessionStartMs = $derived(
		activeSession?.start_time ? new Date(activeSession.start_time).getTime() : null
	);
	/** The replay running for this session, if any: it decides the profile's alignment. */
	const replay = $derived(
		$replayState &&
			$replayState.active &&
			activeSession &&
			$replayState.session_id === activeSession.id
			? $replayState
			: null
	);

	/** The model's live prediction, read only between CHARGE and DROP: outside them a poll's
	 * update rebuilds nothing (the model would drop it anyway). */
	const livePrediction = $derived(
		landmarks.some((l) => l.type === 'charge') && !landmarks.some((l) => l.type === 'drop')
			? predictionState.live
			: null
	);

	const model = $derived.by(() => {
		const unit = unitState.unit; // tracked so the model re-derives on unit change
		void unit;
		return scopeModel({
			history: $telemetryHistory,
			sessionStartMs,
			landmarks,
			previewing,
			profile: profileState.activeProfile,
			replay,
			prediction: livePrediction,
			heaterLine: chartPrefs.heater,
			channels,
			targetDtr: targetDtrSetting,
			view
		});
	});

	// ---- uPlot instance ----
	let wrapper: HTMLDivElement;
	let plotEl: HTMLDivElement;
	let chart: uPlot | null = null;
	let zoomed = $state(false);
	let fullscreen = $state(false);
	let cursor = $state<ScopeCursor | null>(null);

	function buildOptions(width: number, height: number) {
		return scopeOptions({
			width,
			height,
			pal,
			channels,
			visible,
			channelShown: chShown,
			xRange: (u, min, max) =>
				zoomed ? [u.scales.x.min ?? min, u.scales.x.max ?? max] : (model?.xRange ?? [min, max]),
			channelTop: (i) => model?.chLines[i]?.top ?? null,
			hooks: {
				drawClear: phaseBandsHook,
				draw: drawOverlays,
				setCursor: onCursor,
				onZoomSelect: () => {
					zoomed = true;
				}
			}
		});
	}

	function phaseBandsHook(u: uPlot) {
		if (model) drawPhaseBands(u, model.bands, model.pct, pal);
	}

	function drawOverlays(u: uPlot) {
		const m = model;
		if (!m) return;
		const { ctx, bbox } = u;
		const dpr = devicePixelRatio;
		ctx.save();
		ctx.beginPath();
		ctx.rect(bbox.left, bbox.top - 38 * dpr, bbox.width, bbox.height + 38 * dpr);
		ctx.clip();
		drawLandmarks(u, m.marks, pal, (c) => fmtTemp(c, 0));
		drawEventFlags(u, m.flags, visible, pal, (c) => fmtTemp(c, 0));
		drawProfileTicks(u, m.ticks, m.nowSec, pal, (e) =>
			describeEvent(e as ProfileEventInput, (c) => `${fmtTemp(c, 0)}°`)
		);
		drawNowMarker(u, m.nowSec, pal, isLight());
		if (m.stats.dev) drawDropPrediction(u, m.stats.dev);
		if (m.stats.devWindow) drawDevWindow(u, m.stats.devWindow);
		ctx.restore();
	}

	function onCursor(u: uPlot) {
		cursor = model ? cursorAt(model, u.cursor.idx) : null;
	}

	// ---- zoom: wheel, pinch, touch pan ----
	/** The x series: the live samples and the future drawn ahead of them. */
	const xs = () => (model?.data[0] as number[] | undefined) ?? [];

	function zoomAround(u: uPlot, xVal: number, factor: number) {
		const { range, full } = zoomWindow(xs(), [u.scales.x.min!, u.scales.x.max!], xVal, factor);
		zoomed = !full;
		u.setScale('x', { min: range[0], max: range[1] });
	}

	function attachGestures(u: uPlot) {
		const over = u.over;
		over.addEventListener(
			'wheel',
			(e) => {
				e.preventDefault();
				const rect = over.getBoundingClientRect();
				const xVal = u.posToVal(e.clientX - rect.left, 'x');
				zoomAround(u, xVal, e.deltaY < 0 ? 0.85 : 1 / 0.85);
			},
			{ passive: false }
		);
		// eslint-disable-next-line svelte/prefer-svelte-reactivity -- touch bookkeeping for the gesture handlers, never rendered
		const pointers = new Map<number, { x: number }>();
		let pinchStart: { dist: number; min: number; max: number; center: number } | null = null;
		let panStart: { x: number; min: number; max: number } | null = null;
		over.addEventListener('pointerdown', (e) => {
			if (e.pointerType !== 'touch') return;
			pointers.set(e.pointerId, { x: e.clientX });
			if (pointers.size === 2) {
				const [p1, p2] = [...pointers.values()];
				const rect = over.getBoundingClientRect();
				const center = u.posToVal((p1.x + p2.x) / 2 - rect.left, 'x');
				pinchStart = {
					dist: Math.abs(p1.x - p2.x),
					min: u.scales.x.min!,
					max: u.scales.x.max!,
					center
				};
				panStart = null;
			} else if (pointers.size === 1 && zoomed) {
				panStart = { x: e.clientX, min: u.scales.x.min!, max: u.scales.x.max! };
			}
		});
		over.addEventListener('pointermove', (e) => {
			if (e.pointerType !== 'touch') return;
			if (!pointers.has(e.pointerId)) return;
			pointers.set(e.pointerId, { x: e.clientX });
			if (pinchStart && pointers.size === 2) {
				const [p1, p2] = [...pointers.values()];
				const dist = Math.max(10, Math.abs(p1.x - p2.x));
				const factor = pinchStart.dist / dist;
				const [a, b] = pinchWindow(xs(), pinchStart, factor);
				zoomed = true;
				u.setScale('x', { min: a, max: b });
			} else if (panStart && pointers.size === 1) {
				const pxPerVal = over.clientWidth / (panStart.max - panStart.min);
				const dv = (panStart.x - e.clientX) / pxPerVal;
				const [a, b] = panWindow(xs(), panStart, dv);
				u.setScale('x', { min: a, max: b });
			}
		});
		const end = (e: PointerEvent) => {
			pointers.delete(e.pointerId);
			if (pointers.size < 2) pinchStart = null;
			if (pointers.size === 0) panStart = null;
		};
		over.addEventListener('pointerup', end);
		over.addEventListener('pointercancel', end);
		over.addEventListener('dblclick', () => resetZoom());
	}

	function resetZoom() {
		zoomed = false;
		if (chart && model) chart.setData(model.data, true);
	}

	function createChart() {
		if (!plotEl || !model) return;
		const w = plotEl.clientWidth || 600;
		const h = plotEl.clientHeight || 400;
		chart = new uPlot(buildOptions(w, h), model.data, plotEl);
		attachGestures(chart);
	}

	function destroyChart() {
		chart?.destroy();
		chart = null;
	}

	// Narrow (a phone) or not: the axes differ, so crossing the line rebuilds the chart.
	let compactAxes = $state(false);

	onMount(() => {
		const ro = new ResizeObserver(() => {
			if (!plotEl) return;
			compactAxes = axisLayout(plotEl.clientWidth || 600).compact;
			if (chart) chart.setSize({ width: plotEl.clientWidth, height: plotEl.clientHeight });
		});
		ro.observe(plotEl);
		const onFs = () => {
			fullscreen = document.fullscreenElement === wrapper;
			requestAnimationFrame(() =>
				chart?.setSize({ width: plotEl.clientWidth, height: plotEl.clientHeight })
			);
		};
		document.addEventListener('fullscreenchange', onFs);
		return () => {
			ro.disconnect();
			document.removeEventListener('fullscreenchange', onFs);
			destroyChart();
		};
	});

	// Rebuild on theme, unit or channel change (axis labels, palette, series); push data otherwise.
	let rebuildKey = $derived(
		`${isLight() ? 'light' : 'dark'}|${unitState.unit}|${channelSig}|${compactAxes}`
	);
	$effect(() => {
		void rebuildKey;
		const m = untrack(() => model);
		if (!m) return;
		destroyChart();
		createChart();
	});
	$effect(() => {
		const m = model;
		if (!m) return;
		if (!chart) {
			untrack(() => createChart());
			return;
		}
		chart.setData(m.data, !zoomed);
	});

	function toggleFullscreen() {
		if (document.fullscreenElement === wrapper) document.exitFullscreen();
		else wrapper.requestFullscreen?.();
	}

	const readout = $derived(model ? scopeReadout(model, cursor, previewing) : null);
</script>

<div
	bind:this={wrapper}
	class="flex h-full min-h-[340px] flex-col rounded-lg border border-border bg-card {fullscreen
		? 'p-3'
		: ''}"
>
	<ScopeHeader
		{readout}
		cursor={!!cursor}
		{liveReadout}
		{replay}
		stats={model?.stats ?? null}
		modelShown={visible.model}
		{view}
		{hasFrame}
		{zoomed}
		{fullscreen}
		onview={chooseView}
		onreset={resetZoom}
		onfullscreen={toggleFullscreen}
	/>

	<!-- Plot -->
	<div bind:this={plotEl} class="scope-plot relative min-h-0 flex-1">
		{#if !model}
			<div class="absolute inset-0 flex items-center justify-center text-sm text-muted-foreground">
				Waiting for telemetry…
			</div>
		{/if}
	</div>

	<SeriesToggles
		{visible}
		{channels}
		{chVisible}
		hasProfile={!!profileState.activeProfile}
		ontoggle={toggle}
		onchannel={toggleChannel}
	/>
</div>

<style>
	:global(.scope-plot .uplot) {
		font-family: inherit;
	}
	:global(.scope-plot .u-select) {
		background: rgba(245, 158, 11, 0.15);
	}
</style>
