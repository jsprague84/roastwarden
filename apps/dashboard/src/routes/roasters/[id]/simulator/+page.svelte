<script lang="ts">
	/**
	 * A simulator's switch, chamber and model (docs/virtual-roaster.md): the fluid bed's on its
	 * page, the drum's on its own (docs/add-roaster.md). Both switches are on Roasters too.
	 */
	import { isVirtual } from '#lib/roast/live.js';
	import VirtualRoasterPanel from '#lib/components/roast/VirtualRoasterPanel.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const virtual = $derived(isVirtual(data.device.device_id));
</script>

{#if virtual}
	<VirtualRoasterPanel panelKey="simulator-page" show={data.device.device_id} />
{:else}
	<p class="text-sm text-muted-foreground">Only the simulated roasters have a simulator.</p>
{/if}
