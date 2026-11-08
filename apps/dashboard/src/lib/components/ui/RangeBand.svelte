<script lang="ts">
	/**
	 * A value against its expected range (docs/roast-guidance.md): the number, a thin bar with
	 * the range and a marker, and a "?" with the range, its basis and sources. Amber only when
	 * outside, on the number and the marker; within is plain text.
	 */
	import type { Snippet } from 'svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import type { Judgement } from '#lib/guidance/judge.js';
	import { bandGeometry, bandHelp } from '#lib/guidance/band.js';

	let {
		j,
		range,
		level = '',
		extra = [],
		children
	}: {
		j: Judgement | null;
		/** The range in display units, e.g. "14–16 %". */
		range: string;
		/** Level label for "Light expects". */
		level?: string;
		extra?: string[];
		children?: Snippet;
	} = $props();

	const outside = $derived(j?.state === 'below' || j?.state === 'above');
	const g = $derived(j ? bandGeometry(j.range, j.value) : null);
	const h = $derived(j ? bandHelp(j, range, level, extra) : null);
</script>

<span class="inline-flex items-center gap-1.5 align-middle">
	{#if children}<span class={outside ? 'text-amber-400' : ''}>{@render children()}</span>{/if}
	{#if j && g && h}
		<span class="relative inline-block h-2.5 w-14 shrink-0" role="img" aria-label={h.help.line}>
			<span class="absolute inset-x-0 top-1/2 h-px -translate-y-1/2 bg-border"></span>
			<span
				class="absolute top-1/2 h-1 -translate-y-1/2 rounded-sm bg-muted-foreground/60"
				style="left: {g.lo}%; width: {g.hi - g.lo}%"
			></span>
			{#if g.at != null}
				<span
					class="absolute top-0 h-2.5 w-[2px] -translate-x-1/2 rounded-sm {outside
						? 'bg-amber-500'
						: 'bg-foreground'}"
					style="left: {g.at}%"
				></span>
			{/if}
		</span>
		<Hint inline help={h.help} links={h.links} />
	{/if}
</span>
