<script lang="ts">
	/**
	 * A roaster against the interface standard (docs/add-roaster.md, "roast roaster check"): the
	 * verdict, a line per check and the reading now, refreshed while it is shown. The roaster's
	 * Check tab and the Add roaster wizard's Check step. Only reads.
	 */
	import { Check, TriangleAlert, CircleX, Loader2, RefreshCw } from '@lucide/svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { HELP } from '#lib/constants/help.js';
	import { roasterCheck, type RoasterCheck, type CheckStatus } from '#lib/api/adding.js';
	import {
		checkLabel,
		orderedLines,
		readingItems,
		statusClass,
		verdictText
	} from '#lib/roaster/check.js';
	import { fmtTemp, tempUnit } from '#lib/stores/units.svelte.js';

	let {
		deviceId,
		every = 5000,
		title = 'Check',
		view = $bindable(null)
	}: { deviceId: string; every?: number; title?: string; view?: RoasterCheck | null } = $props();

	let error = $state('');
	let loading = $state(false);

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	async function refresh(id: string) {
		loading = true;
		try {
			const v = await roasterCheck.get(id);
			if (id === deviceId) {
				view = v;
				error = '';
			}
		} catch (e) {
			if (id === deviceId) error = msg(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const id = deviceId;
		view = null;
		void refresh(id);
		const t = setInterval(() => void refresh(id), every);
		return () => clearInterval(t);
	});

	const lines = $derived(view ? orderedLines(view.lines) : []);
	const reading = $derived(readingItems(view?.reading));

	function show(item: { kind: string; value: number }): string {
		if (item.kind === 'temp') return `${fmtTemp(item.value, 1)}${tempUnit()}`;
		if (item.kind === 'pct') return `${item.value.toFixed(0)} %`;
		return `${Math.round(item.value * 100) / 100}`;
	}
</script>

{#snippet mark(s: CheckStatus)}
	{#if s === 'ok'}<Check class="mt-0.5 h-3.5 w-3.5 shrink-0 {statusClass(s)}" />
	{:else if s === 'warn'}<TriangleAlert class="mt-0.5 h-3.5 w-3.5 shrink-0 {statusClass(s)}" />
	{:else}<CircleX class="mt-0.5 h-3.5 w-3.5 shrink-0 {statusClass(s)}" />{/if}
{/snippet}

<div>
	<div class="flex items-center justify-between gap-3">
		<div class="flex items-center gap-2">
			<h2 class="text-lg font-semibold text-foreground">{title}</h2>
			<Hint help={HELP.roasterCheck} inline />
		</div>
		<button
			type="button"
			onclick={() => refresh(deviceId)}
			disabled={loading}
			class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-50"
		>
			<RefreshCw class="h-3.5 w-3.5 {loading ? 'animate-spin' : ''}" />
			Refresh
		</button>
	</div>

	{#if error}
		<p class="mt-3 text-sm text-destructive">{error}</p>
	{:else if !view}
		<p class="mt-3 flex items-center gap-2 text-sm text-muted-foreground">
			<Loader2 class="h-4 w-4 animate-spin" /> Checking…
		</p>
	{:else}
		<p
			class="mt-3 flex items-start gap-1.5 text-sm font-medium {statusClass(view.verdict)}"
			data-testid="check-verdict"
		>
			{@render mark(view.verdict)}
			{verdictText(view.verdict)}
			<span class="font-normal text-muted-foreground">
				· {view.samples} samples in 30 s{view.rate_hz != null
					? `, ${view.rate_hz.toFixed(2)} a second`
					: ''}
			</span>
		</p>
		<dl class="mt-3 grid grid-cols-[6rem_1fr] gap-x-4 gap-y-1.5 text-sm sm:grid-cols-[8rem_1fr]">
			{#each lines as l (l.check)}
				<dt class="text-muted-foreground">{checkLabel(l.check)}</dt>
				<dd class="flex items-start gap-1.5 text-foreground">
					{@render mark(l.status)}{l.message}
				</dd>
			{/each}
		</dl>
		{#if reading.length > 0}
			<h3 class="mt-4 text-sm font-medium text-foreground">Reading now</h3>
			<div class="mt-1 flex flex-wrap gap-x-5 gap-y-1 text-sm">
				{#each reading as item (item.key)}
					<span class="text-muted-foreground"
						>{item.label} <span class="tabular-nums text-foreground">{show(item)}</span></span
					>
				{/each}
			</div>
		{/if}
	{/if}
</div>
