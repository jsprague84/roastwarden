<script lang="ts">
	/**
	 * Small live chart for an autotune run: BT and the heater % the relay
	 * applies, with the test temperature as a reference line. Built from the
	 * telemetry history the dashboard already keeps, so it survives a reload.
	 */
	import uPlot, { type AlignedData, type Options } from 'uplot';
	import 'uplot/dist/uPlot.min.css';
	import { onMount } from 'svelte';
	import { telemetryHistory } from '#lib/stores/telemetry.js';
	import { chartPalette } from '#lib/constants/chartTheme.js';
	import { unitState, toDisplay, tempUnit } from '#lib/stores/units.svelte.js';
	import { isLight } from '#lib/stores/theme.svelte.js';
	import { sampleToTelemetry, type Sample } from '#lib/telemetry/sample.js';
	import type { Telemetry } from '#lib/api/telemetry.js';

	let {
		deviceId,
		fromMs,
		testTemp = null,
		height = 170
	}: { deviceId: string; fromMs: number; testTemp?: number | null; height?: number } = $props();

	type Point = { timestamp: number; telemetry: Telemetry };
	// The browser's history starts when the page connected; after a reload
	// mid-run the server's sample ring fills in the start of the test.
	let seeded = $state<Point[]>([]);
	$effect(() => {
		const id = deviceId;
		const since = Math.floor(fromMs);
		if (!id) return;
		let cancelled = false;
		const base = import.meta.env.VITE_API_URL ?? '';
		fetch(`${base}/api/roaster/${encodeURIComponent(id)}/samples?since_ms=${since}`)
			.then((r) => (r.ok ? r.json() : null))
			.then((body: { samples?: Sample[] } | null) => {
				if (cancelled || !body?.samples) return;
				seeded = body.samples.map((x) => ({ timestamp: x.t, telemetry: sampleToTelemetry(x) }));
			})
			.catch(() => {});
		return () => {
			cancelled = true;
		};
	});

	const BT = '#f59e0b';
	const HEATER = '#ef4444';
	const TEST = '#60a5fa';

	let el: HTMLDivElement | undefined = $state();
	let chart: uPlot | null = null;

	function data(): AlignedData {
		const lastSeeded = seeded.length ? seeded[seeded.length - 1].timestamp : -Infinity;
		const h = [...seeded, ...$telemetryHistory.filter((p) => p.timestamp > lastSeeded)].filter(
			(p) => p.timestamp >= fromMs
		);
		const t0 = h.length ? h[0].timestamp : fromMs;
		const t: number[] = [];
		const bt: (number | null)[] = [];
		const heat: (number | null)[] = [];
		const ref: (number | null)[] = [];
		const test = testTemp != null ? toDisplay(testTemp) : null;
		for (const p of h) {
			const tm = p.telemetry;
			t.push((p.timestamp - t0) / 1000);
			bt.push(Number.isFinite(tm.beanTemp) ? toDisplay(tm.beanTemp) : null);
			const applied = tm.heaterApplied ?? tm.heater;
			heat.push(Number.isFinite(applied) ? applied : null);
			ref.push(test);
		}
		return [t, bt, heat, ref];
	}

	function mmss(s: number): string {
		const m = Math.floor(s / 60);
		return `${m}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
	}

	function options(width: number): Options {
		const pal = chartPalette();
		const font = `11px ${pal.font}`;
		const grid = { stroke: pal.line, width: 1 };
		return {
			width,
			height,
			legend: { show: false },
			cursor: { drag: { x: false, y: false } },
			scales: { x: { time: false }, temp: {}, pct: { range: [0, 100] } },
			axes: [
				{ stroke: pal.axisText, grid, ticks: grid, font, values: (_u, v) => v.map(mmss) },
				{
					scale: 'temp',
					stroke: BT,
					grid,
					ticks: grid,
					font,
					size: 44,
					label: tempUnit(),
					labelFont: font,
					labelSize: 12
				},
				{
					scale: 'pct',
					side: 1,
					stroke: HEATER,
					grid: { show: false },
					ticks: grid,
					font,
					size: 36,
					label: '%',
					labelFont: font,
					labelSize: 12
				}
			],
			series: [
				{},
				{ label: 'BT', scale: 'temp', stroke: BT, width: 2, points: { show: false } },
				{
					label: 'Heater',
					scale: 'pct',
					stroke: HEATER,
					width: 1.5,
					paths: uPlot.paths.stepped!({ align: 1 }),
					points: { show: false }
				},
				{
					label: 'Test',
					scale: 'temp',
					stroke: TEST,
					width: 1,
					dash: [6, 4],
					points: { show: false }
				}
			]
		};
	}

	function build() {
		chart?.destroy();
		chart = null;
		if (!el) return;
		chart = new uPlot(options(el.clientWidth || 300), data(), el);
	}

	onMount(() => {
		build();
		const ro = new ResizeObserver(() => {
			if (chart && el) chart.setSize({ width: el.clientWidth, height });
		});
		if (el) ro.observe(el);
		return () => {
			ro.disconnect();
			chart?.destroy();
			chart = null;
		};
	});

	// Rebuild on theme or unit change; otherwise just feed new data.
	let key = $derived(`${isLight() ? 'l' : 'd'}|${unitState.unit}`);
	let lastKey = '';
	$effect(() => {
		const k = key;
		if (lastKey && k !== lastKey) build();
		lastKey = k;
	});
	$effect(() => {
		const d = data();
		chart?.setData(d);
	});
</script>

<div class="w-full" bind:this={el} style="height: {height}px"></div>
<div class="mt-1 flex flex-wrap gap-3 text-[11px] text-muted-foreground">
	<span><span class="inline-block h-0.5 w-3 align-middle" style="background:{BT}"></span> BT</span>
	<span
		><span class="inline-block h-0.5 w-3 align-middle" style="background:{HEATER}"></span> Heater %</span
	>
	{#if testTemp != null}
		<span
			><span class="inline-block h-0.5 w-3 align-middle" style="background:{TEST}"></span> Test temperature</span
		>
	{/if}
</div>
