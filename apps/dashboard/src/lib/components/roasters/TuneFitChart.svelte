<script lang="ts">
	/**
	 * A server autotune's record against its fitted model (docs/autotune-2.md): BT as measured,
	 * the model's BT dashed, and the heater as applied underneath. Where the two BT lines
	 * agree, the model (and the designs built on it) can be trusted.
	 */
	import type { TuneSamples } from '#lib/api/tuning.js';
	import { toDisplay, tempUnit } from '#lib/stores/units.svelte.js';

	let { samples, height = 190 }: { samples: TuneSamples; height?: number } = $props();

	// Drawn at the width it is shown at, so its text stays small on a wide screen and readable on a phone.
	let cw = $state(640);
	const W = $derived(Math.max(cw, 280));
	const padL = 44;
	const padR = 34;
	const top = 8;
	const btH = $derived(height * 0.62);
	const heatTop = $derived(top + btH + 14);
	const heatH = $derived(height - heatTop - 18);

	// The relay part (from 30 s before the model starts) is what the fit covers.
	const from = $derived(
		samples.model?.t[0] != null
			? Math.max(samples.t[0] ?? 0, samples.model.t[0] - 30)
			: (samples.t[0] ?? 0)
	);
	const to = $derived(samples.t[samples.t.length - 1] ?? from + 1);
	const idx = $derived(samples.t.map((t, i) => (t >= from ? i : -1)).filter((i) => i >= 0));
	const ys = $derived(
		[...idx.map((i) => samples.y[i]), ...(samples.model?.y ?? [])].filter(Number.isFinite)
	);
	const yMin = $derived(Math.floor(Math.min(...ys) - 0.5));
	const yMax = $derived(Math.ceil(Math.max(...ys) + 0.5));

	const x = (t: number) => padL + ((t - from) / Math.max(to - from, 1)) * (W - padL - padR);
	const yBt = (v: number) => top + (1 - (v - yMin) / Math.max(yMax - yMin, 0.1)) * btH;
	const yHeat = (u: number) => heatTop + (1 - u / 100) * heatH;

	function line(ts: number[], vs: number[], y: (v: number) => number): string {
		let d = '';
		ts.forEach((t, i) => {
			if (t < from || !Number.isFinite(vs[i])) return;
			d += `${d ? 'L' : 'M'}${x(t).toFixed(1)},${y(vs[i]).toFixed(1)}`;
		});
		return d;
	}

	/** The heater holds from one sample to the next: a step line. */
	const heaterPath = $derived.by(() => {
		let d = '';
		idx.forEach((i, k) => {
			const t0 = samples.t[i];
			const t1 = samples.t[idx[k + 1]] ?? t0 + 1;
			const yy = yHeat(samples.u[i]).toFixed(1);
			d += `${d ? 'L' : 'M'}${x(t0).toFixed(1)},${yy}L${x(t1).toFixed(1)},${yy}`;
		});
		return d;
	});

	const btTicks = $derived([yMin, (yMin + yMax) / 2, yMax]);
	const minutes = $derived(
		Array.from({ length: Math.floor((to - from) / 60) + 1 }, (_, i) => from + i * 60)
	);
	const labelEvery = $derived(W < 420 ? 2 : 1);
</script>

<div bind:clientWidth={cw} class="w-full">
	<svg
		width={W}
		{height}
		viewBox="0 0 {W} {height}"
		class="block"
		role="img"
		aria-label="Autotune record against the fitted model"
	>
		{#each btTicks as v (v)}
			<line
				x1={padL}
				x2={W - padR}
				y1={yBt(v)}
				y2={yBt(v)}
				class="stroke-border"
				stroke-width="0.5"
			/>
			<text x={padL - 4} y={yBt(v) + 3} text-anchor="end" class="fill-muted-foreground text-[9px]"
				>{toDisplay(v).toFixed(0)}{v === yMax ? tempUnit() : ''}</text
			>
		{/each}
		{#each minutes as m, i (m)}
			<line
				x1={x(m)}
				x2={x(m)}
				y1={top}
				y2={height - 16}
				class="stroke-border"
				stroke-width="0.4"
			/>
			{#if i % labelEvery === 0}<text
					x={x(m)}
					y={height - 4}
					text-anchor="middle"
					class="fill-muted-foreground text-[9px]">{i} min</text
				>{/if}
		{/each}
		<line
			x1={padL}
			x2={W - padR}
			y1={yHeat(100)}
			y2={yHeat(100)}
			class="stroke-border"
			stroke-width="0.5"
			stroke-dasharray="2 3"
		/>
		<text x={W - padR + 4} y={yHeat(100) + 3} class="fill-muted-foreground text-[9px]">100 %</text>
		<text x={W - padR + 4} y={yHeat(0) + 3} class="fill-muted-foreground text-[9px]">0 %</text>

		<path
			d={heaterPath}
			fill="none"
			stroke="currentColor"
			stroke-width="1"
			class="text-amber-400"
		/>
		<path
			d={line(samples.t, samples.y, yBt)}
			fill="none"
			stroke="currentColor"
			stroke-width="1.4"
			class="text-red-400"
		/>
		{#if samples.model}
			<path
				d={line(samples.model.t, samples.model.y, yBt)}
				fill="none"
				stroke="currentColor"
				stroke-width="1.4"
				stroke-dasharray="5 3"
				class="text-sky-400"
			/>
		{/if}
	</svg>
</div>
<div class="flex flex-wrap gap-x-4 text-[11px] text-muted-foreground">
	<span><span class="text-red-400">━</span> BT measured</span>
	{#if samples.model}<span><span class="text-sky-400">┅</span> the model</span>{/if}
	<span><span class="text-amber-400">━</span> heater</span>
</div>
