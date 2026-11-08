<script lang="ts">
	/**
	 * A roaster against the device contract (docs/control.md): its firmware, the readings it
	 * sends, what it can do, and whether its PID gains are the ones the server holds, with the
	 * last check after a boot. Only reads.
	 */
	import { onMount } from 'svelte';
	import { Check, TriangleAlert } from '@lucide/svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { HELP } from '#lib/constants/help.js';
	import { contract, type ContractView } from '#lib/api/roasters.js';
	import { contractLines } from '#lib/roaster/contract.js';

	let { deviceId }: { deviceId: string } = $props();
	let view = $state<ContractView | null>(null);
	let error = $state('');

	onMount(async () => {
		try {
			view = await contract.get(deviceId);
		} catch (e) {
			error =
				e instanceof Error
					? e.message.replace(/^API error \d+: /, '')
					: 'Not heard since the server started';
		}
	});

	let lines = $derived(view ? contractLines(view) : null);
</script>

<div class="rounded-lg border border-border bg-card p-6">
	<div class="flex items-center gap-2">
		<h2 class="text-lg font-semibold text-foreground">Device contract</h2>
		<Hint help={HELP.deviceContract} inline />
	</div>
	{#if lines && view}
		<dl class="mt-3 grid grid-cols-[5.5rem_1fr] sm:grid-cols-[8rem_1fr] gap-x-4 gap-y-1.5 text-sm">
			<dt class="text-muted-foreground">Firmware</dt>
			<dd class="text-foreground">{view.firmware ?? 'unknown'}</dd>
			<dt class="text-muted-foreground">Readings</dt>
			<dd class="flex items-start gap-1.5 text-foreground">
				{#if lines.reads.ok}<Check
						class="mt-0.5 h-3.5 w-3.5 shrink-0 text-go"
					/>{:else}<TriangleAlert class="mt-0.5 h-3.5 w-3.5 shrink-0 text-highlight" />{/if}
				{lines.reads.text}
			</dd>
			<dt class="text-muted-foreground">Can do</dt>
			<dd class="text-foreground">{lines.caps}</dd>
			<dt class="text-muted-foreground">PID gains</dt>
			<dd class="flex items-start gap-1.5 text-foreground" data-testid="contract-gains">
				{#if lines.gains.ok === true}<Check
						class="mt-0.5 h-3.5 w-3.5 shrink-0 text-go"
					/>{:else if lines.gains.ok === false}<TriangleAlert
						class="mt-0.5 h-3.5 w-3.5 shrink-0 text-highlight"
					/>{/if}
				{lines.gains.text}
			</dd>
			{#if lines.boot}
				<dt class="text-muted-foreground">Last boot</dt>
				<dd class="text-foreground">{lines.boot}</dd>
			{/if}
		</dl>
	{:else if error}
		<p class="mt-3 text-sm text-muted-foreground">{error}</p>
	{/if}
</div>
