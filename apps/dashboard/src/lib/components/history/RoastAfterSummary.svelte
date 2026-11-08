<script lang="ts">
	/**
	 * Under "Saved: …" on the Roast page (docs/roast-guidance.md): the roasted weight, then the
	 * roast summary once the weights are in. Nothing off, no lines.
	 */
	import { events as eventsApi } from '#lib/api/events.js';
	import { profiles } from '#lib/api/profiles.js';
	import { sessions, type RoastSession } from '#lib/api/sessions.js';
	import { resolve } from '$app/paths';
	import { readRoast } from '#lib/guidance/summary.js';
	import { fmtDelta, fmtTemp, rorUnit, tempUnit } from '#lib/stores/units.svelte.js';
	import { fmtGrams } from '#lib/beans/stock.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';
	import { notifications } from '#lib/stores/notifications.js';
	import GuidanceSummary from '#lib/components/history/GuidanceSummary.svelte';
	import { isOn } from '#lib/stores/modules.svelte.js';

	let { session, onsaved }: { session: RoastSession; onsaved?: (s: RoastSession) => void } =
		$props();

	let weight = $state<number | null>(null);
	let green = $state<number | null>(null);
	let saving = $state(false);
	let lines = $state<string[]>([]);

	async function read(s: RoastSession) {
		if (s.green_weight == null || s.roasted_weight == null) {
			lines = [];
			return;
		}
		try {
			const [full, evs, history] = await Promise.all([
				sessions.get(s.id),
				eventsApi.list(s.id),
				s.profile_id
					? profiles.history(s.profile_id, s.device_id).catch(() => [])
					: Promise.resolve([])
			]);
			lines = readRoast({
				session: full,
				events: evs,
				telemetry: full.telemetry ?? [],
				history,
				fmt: {
					temp: (c) => `${fmtTemp(c)} ${tempUnit()}`,
					rate: (c) => `${fmtDelta(c, 0)} ${rorUnit()}`
				}
			}).lines;
		} catch {
			lines = [];
		}
	}
	$effect(() => {
		void read(session);
	});

	async function save(e: SubmitEvent) {
		e.preventDefault();
		const r = weight;
		const g = session.green_weight ?? green;
		if (r == null || r <= 0 || g == null || g <= 0) return;
		saving = true;
		try {
			const updated = await sessions.update(session.id, {
				roasted_weight: r,
				...(session.green_weight == null ? { green_weight: g } : {})
			});
			onsaved?.(updated);
		} catch (err) {
			notifications.add(
				`Could not save the weight: ${err instanceof Error ? err.message : err}`,
				'error'
			);
		} finally {
			saving = false;
		}
	}
</script>

{#if session.roasted_weight == null}
	<form class="mb-3 flex items-center gap-2 text-xs" onsubmit={save}>
		{#if session.green_weight == null}
			<label for="after-green" class="text-muted-foreground">Green</label>
			<WeightInput
				id="after-green"
				bind:grams={green}
				class="w-16 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
			/>
		{/if}
		<label for="after-weight" class="text-muted-foreground">Roasted weight</label>
		<WeightInput
			id="after-weight"
			bind:grams={weight}
			class="w-16 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
		/>
		{#if session.green_weight != null}<span class="text-muted-foreground"
				>of {fmtGrams(session.green_weight, { precise: true })}</span
			>{/if}
		<button
			type="submit"
			disabled={saving}
			class="ml-auto rounded-md border border-border px-2.5 py-1 text-foreground hover:bg-accent disabled:opacity-50"
			>{saving ? '…' : 'Save'}</button
		>
	</form>
{/if}
<GuidanceSummary
	lines={lines.slice(0, 1)}
	compact
	class="-mt-1.5 text-xs {lines.length > 1 || session.roasted_weight != null ? '' : 'mb-2'}"
/>
{#if lines.length > 1 || session.roasted_weight != null}
	<!-- One line: the rest of the summary, then the bag labels and the report (once weighed,
	     the weight form's line is free for it) -->
	<div class="mb-2 flex flex-wrap gap-x-3 pl-3.5 text-[11px] text-muted-foreground">
		{#if lines.length > 1}
			<a
				href={resolve('/history/[id]', { id: session.id })}
				class="hover:text-foreground hover:underline"
				>and {lines.length - 1} more on the roast's page</a
			>
		{/if}
		{#if session.roasted_weight != null}
			{#if isOn('labels')}<a
					href={resolve('/history/[id]/labels', { id: session.id })}
					class="text-action hover:underline"
					title="Bag labels for this roast">Labels</a
				>{/if}
			{#if isOn('reports')}<a
					href={resolve(`/reports?kind=roast&roast=${encodeURIComponent(session.id)}`)}
					class="text-action hover:underline"
					title="This roast's report as a PDF">Report</a
				>{/if}
		{/if}
	</div>
{/if}
