<script lang="ts">
	/**
	 * A roaster against the interface standard (docs/add-roaster.md, "roast roaster check"):
	 * the read-only check (CheckView, refreshed every 5 s), and for the owner the write check
	 * (heater 10 %, then emergency stop) behind a confirmation that names what it sends.
	 */
	import { Check, CircleX, Loader2 } from '@lucide/svelte';
	import CheckView from '#lib/components/roasters/CheckView.svelte';
	import { roasterCheck, type WriteCheck } from '#lib/api/adding.js';
	import { auth, isOwnerCredential } from '#lib/api/auth.js';
	import { statusClass, writeSummary, WRITE_CHECK_CONFIRM } from '#lib/roaster/check.js';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const deviceId = $derived(data.device.device_id as string);

	let owner = $state(false);
	let writing = $state(false);
	let written = $state<WriteCheck | null>(null);
	let writeError = $state('');
	/** Bumped after the write check, so the read check shows what it left. */
	let round = $state(0);

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	$effect(() => {
		void deviceId;
		written = null;
		writeError = '';
		void auth
			.status()
			.then((s) => (owner = isOwnerCredential(s.credential) || s.credential === 'open'))
			.catch(() => (owner = false));
	});

	async function runWrite() {
		if (!confirm(`${WRITE_CHECK_CONFIRM}\n\nRoaster: ${deviceId}`)) return;
		writing = true;
		writeError = '';
		written = null;
		try {
			written = await roasterCheck.write(deviceId);
		} catch (e) {
			writeError = msg(e);
		} finally {
			writing = false;
			round++;
		}
	}
</script>

{#snippet mark(ok: boolean)}
	{#if ok}<Check class="mt-0.5 h-3.5 w-3.5 shrink-0 {statusClass('ok')}" />
	{:else}<CircleX class="mt-0.5 h-3.5 w-3.5 shrink-0 {statusClass('fail')}" />{/if}
{/snippet}

<div class="space-y-6">
	<div class="rounded-lg border border-border bg-card p-6">
		{#key round}<CheckView {deviceId} />{/key}
	</div>

	<div class="rounded-lg border border-border bg-card p-6">
		<h2 class="text-lg font-semibold text-foreground">Write check</h2>
		<p class="mt-1 text-sm text-muted-foreground">
			Does the roaster obey and stop? It sends heater 10 % for up to 5 s, waits for the heater's
			echo, then sends emergency stop and waits for the heater to read 0. Always ends with emergency
			stop. Cold roaster only (BT below 50 °C), no roast open, control manual or full.
		</p>
		{#if owner}
			<button
				type="button"
				onclick={runWrite}
				disabled={writing}
				class="mt-3 inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
			>
				{#if writing}<Loader2 class="h-3.5 w-3.5 animate-spin" />{/if}
				Run the write check
			</button>
		{:else}
			<p class="mt-3 text-sm text-muted-foreground">
				The owner key's alone: sign in with it to run it.
			</p>
		{/if}
		{#if writeError}
			<p class="mt-3 text-sm text-destructive">{writeError}</p>
		{/if}
		{#if written}
			<p
				class="mt-3 flex items-start gap-1.5 text-sm font-medium {statusClass(
					written.ok ? 'ok' : 'fail'
				)}"
			>
				{@render mark(written.ok)}{writeSummary(written)}
			</p>
			<ol class="mt-2 space-y-1 text-sm">
				{#each written.steps as s, i (i)}
					<li class="flex items-start gap-1.5">
						{@render mark(s.ok)}
						<span class="w-14 shrink-0 tabular-nums text-muted-foreground"
							>{(s.at_ms / 1000).toFixed(1)} s</span
						>
						<span class="text-foreground"
							>{s.step}: <span class="text-muted-foreground">{s.message}</span></span
						>
					</li>
				{/each}
			</ol>
		{/if}
	</div>
</div>
