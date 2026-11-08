<script lang="ts">
	/**
	 * The series toggles under the scope: each fixed series (the profile's only with a
	 * profile picked) and each declared channel (docs/channels.md), with its colour and unit.
	 */
	import { channelColor } from '#lib/roaster/channels.js';
	import { SERIES_COLORS, SERIES_LABELS } from '#lib/scope/draw.js';
	import { TOGGLEABLE, type SeriesKey } from '#lib/scope/model.js';
	import type { ScopeChannel } from '#lib/scope/scopeData.js';
	import { rorUnit } from '#lib/stores/units.svelte.js';

	let {
		visible,
		channels,
		chVisible,
		hasProfile,
		ontoggle,
		onchannel
	}: {
		visible: Record<SeriesKey, boolean>;
		channels: ScopeChannel[];
		/** A channel hidden is `false` here; absent is shown. */
		chVisible: Record<string, boolean>;
		/** A profile is picked: its series can be toggled. */
		hasProfile: boolean;
		ontoggle: (k: SeriesKey) => void;
		onchannel: (i: number, key: string) => void;
	} = $props();

	const chShown = (key: string) => chVisible[key] !== false;
</script>

<div class="flex flex-wrap items-center gap-1 border-t border-border px-2 py-1.5">
	{#each TOGGLEABLE as k (k)}
		{#if (k !== 'profBt' && k !== 'profEt' && k !== 'profRor') || hasProfile}
			<button
				type="button"
				class="flex items-center gap-1.5 rounded px-2 py-0.5 text-[11px] transition-colors {visible[
					k
				]
					? 'text-foreground'
					: 'text-muted-foreground/60 line-through'} hover:bg-accent"
				aria-pressed={visible[k]}
				onclick={() => ontoggle(k)}
			>
				<span
					class="inline-block h-2 w-2 rounded-full"
					style="background:{SERIES_COLORS[k]}; opacity:{visible[k] ? 1 : 0.35}"
				></span>
				{SERIES_LABELS[k]}
				{#if k === 'rorBt' || k === 'rorEt' || k === 'projRor' || k === 'profRor'}<span
						class="text-muted-foreground">({rorUnit()})</span
					>{/if}
			</button>
		{/if}
	{/each}
	{#each channels as c, i (c.key)}
		<button
			type="button"
			class="flex items-center gap-1.5 rounded px-2 py-0.5 text-[11px] transition-colors {chShown(
				c.key
			)
				? 'text-foreground'
				: 'text-muted-foreground/60 line-through'} hover:bg-accent"
			aria-pressed={chShown(c.key)}
			title="{c.key}: a declared channel (Roasters → Setup → Channels)"
			onclick={() => onchannel(i, c.key)}
		>
			<span
				class="inline-block h-2 w-2 rounded-full"
				style="background:{channelColor(i)}; opacity:{chShown(c.key) ? 1 : 0.35}"
			></span>
			{c.label}
			{#if c.unit !== 'C' && c.unit !== '%'}<span class="text-muted-foreground">({c.unit})</span
				>{/if}
		</button>
	{/each}
	<span class="ml-auto hidden text-[11px] text-muted-foreground sm:inline"
		>drag or wheel to zoom · pinch on touch · double-click for live</span
	>
</div>
