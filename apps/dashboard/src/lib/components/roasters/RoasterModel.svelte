<script lang="ts">
	/**
	 * A roaster's thermal model (docs/prediction.md): how well it predicts BT two minutes
	 * ahead (each roast left out of the fit in turn) beside today's projection, where BT
	 * settles at full heater, and how the live prediction did on the recent roasts. Refit
	 * takes the roast preset; reading needs none.
	 */
	import { onMount } from 'svelte';
	import { RefreshCw } from '@lucide/svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { HELP } from '#lib/constants/help.js';
	import { prediction, type ModelView } from '#lib/api/predict.js';
	import { modelLines, scoreText, rorScoreText } from '#lib/scope/prediction.js';

	let { deviceId }: { deviceId: string } = $props();
	let view = $state<ModelView | null>(null);
	let error = $state('');
	let fitting = $state(false);

	onMount(async () => {
		try {
			view = await prediction.model(deviceId);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Could not read the model';
		}
	});

	async function refit() {
		fitting = true;
		error = '';
		try {
			view = await prediction.fit(deviceId);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Could not fit the model';
		} finally {
			fitting = false;
		}
	}

	let lines = $derived(view ? modelLines(view) : null);
</script>

<div class="rounded-lg border border-border bg-card p-6" data-testid="roaster-model">
	<div class="flex items-center gap-2">
		<h2 class="text-lg font-semibold text-foreground">Model</h2>
		<Hint help={HELP.roasterModel} inline />
		<button
			type="button"
			class="ml-auto inline-flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs text-muted-foreground hover:text-foreground disabled:opacity-50"
			onclick={refit}
			disabled={fitting}
			title="Fit it again from this roaster's completed roasts"
		>
			<RefreshCw class="h-3.5 w-3.5 {fitting ? 'animate-spin' : ''}" />
			Refit
		</button>
	</div>
	{#if lines && view}
		<p class="mt-2 text-sm text-muted-foreground">{lines.fitted}</p>
		{#if lines.quality.length}
			<table class="mt-3 text-sm">
				<caption class="sr-only">BT two minutes ahead</caption>
				<thead>
					<tr class="text-left text-xs text-muted-foreground">
						<th class="pr-6 font-normal">BT two minutes ahead</th>
						<th class="pr-4 font-normal">90 % within</th>
						<th class="font-normal">RMS</th>
					</tr>
				</thead>
				<tbody>
					{#each lines.quality as q (q.what)}
						<tr>
							<td class="pr-6 text-foreground">{q.what}</td>
							<td class="pr-4 tabular-nums">±{q.p90.toFixed(1)} °C</td>
							<td class="tabular-nums">{q.rmse.toFixed(1)} °C</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
		<p class="mt-3 text-sm">
			<span class="text-muted-foreground">BT settles at heater/fan:</span>
			{lines.settles}
		</p>
		{#if lines.live}<p class="mt-2 text-sm text-foreground">{lines.live}</p>{/if}
		{#if view.recent.length}
			<ul class="mt-2 space-y-0.5 text-xs text-muted-foreground">
				{#each view.recent.slice(0, 5) as r (r.session_id)}
					{@const t = scoreText(r)}
					{@const ror = rorScoreText(r)}
					{#if t || ror}<li>
							{r.batch_number != null ? `#${r.batch_number} ` : ''}{r.name}: {[t, ror]
								.filter(Boolean)
								.join('; ')}
						</li>{/if}
				{/each}
			</ul>
		{/if}
	{:else if error}
		<p class="mt-3 text-sm text-muted-foreground">{error}</p>
	{/if}
	{#if error && view}<p class="mt-2 text-sm text-muted-foreground">{error}</p>{/if}
</div>
