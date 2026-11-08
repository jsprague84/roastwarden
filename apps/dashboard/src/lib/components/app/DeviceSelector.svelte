<script lang="ts">
	/** Roaster picker: the selection is global (roast store), every page shares it. */
	import { resolve } from '$app/paths';
	import { Monitor } from '@lucide/svelte';
	import { roast, selectDevice } from '#lib/stores/roast.svelte.js';

	let { compact = false }: { compact?: boolean } = $props();

	const active = $derived(roast.devices.filter((d) => d.status === 'active'));
	const discovered = $derived(roast.devices.filter((d) => d.status !== 'active'));
</script>

<div class="flex min-w-0 items-center gap-2">
	{#if !compact}<Monitor class="h-4 w-4 shrink-0 text-muted-foreground" />{/if}
	{#if !roast.devicesLoaded}
		<span class="text-sm text-muted-foreground">Loading…</span>
	{:else if roast.devices.length === 0}
		<a href={resolve('/roasters')} class="text-sm text-muted-foreground hover:text-foreground"
			>No roasters</a
		>
	{:else}
		<select
			value={roast.deviceId ?? ''}
			onchange={(e) => selectDevice(e.currentTarget.value || null)}
			aria-label="Roaster"
			class="min-w-0 max-w-[12rem] truncate rounded-md border border-border bg-card py-1.5 pl-2 pr-7 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-focus"
		>
			{#each active as d (d.id)}
				<option value={d.device_id}>{d.name}</option>
			{/each}
			{#if discovered.length > 0}
				<optgroup label="Discovered (not yet activated)">
					{#each discovered as d (d.id)}
						<option value={d.device_id}>{d.name}</option>
					{/each}
				</optgroup>
			{/if}
		</select>
	{/if}
</div>
