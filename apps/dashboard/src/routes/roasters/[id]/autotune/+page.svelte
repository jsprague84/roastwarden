<script lang="ts">
	/** Relay autotune for the roaster, its live chart and results, and the tuning history.
	 * The test drives the heater, so it needs full control (docs/roaster-setup.md). */
	import { resolve } from '$app/paths';
	import { HELP } from '#lib/constants/help.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import AutotunePanel from '#lib/components/roasters/AutotunePanel.svelte';
	import TuningHistory from '#lib/components/roasters/TuningHistory.svelte';
	import { controlLevel } from '#lib/stores/roast.svelte.js';
	import { needsLine } from '#lib/roaster/setup.js';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// The layout shows this page only for the selected roaster, whose setup the store holds.
	const blocked = $derived(needsLine('autotune', controlLevel()));
</script>

<div class="space-y-6">
	<div class="rounded-lg border border-border bg-card p-4">
		<Hint help={HELP.autotune} />
	</div>
	{#if blocked}
		<p class="text-sm text-muted-foreground">
			{blocked}
			<a
				href={resolve('/roasters/[id]/control', { id: data.device.id })}
				class="font-medium text-foreground underline underline-offset-2"
				>Change it in Control → Setup</a
			>
		</p>
	{:else}
		<AutotunePanel />
	{/if}
	<TuningHistory />
</div>
