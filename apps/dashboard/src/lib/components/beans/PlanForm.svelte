<script lang="ts">
	/** Add or edit a planned roast (docs/batches.md). */
	import type { Bean } from '#lib/api/beans.js';
	import type { PlannedRoast, PlanRequest } from '#lib/api/plans.js';
	import type { RoastProfile } from '#lib/api/profiles.js';
	import { roast } from '#lib/stores/roast.svelte.js';
	import { fmtGrams } from '#lib/beans/stock.js';
	import { weightUnit } from '#lib/stores/units.svelte.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';

	let {
		plan = null,
		beans,
		profiles,
		busy = false,
		onsave,
		oncancel
	}: {
		plan?: PlannedRoast | null;
		beans: Bean[];
		profiles: RoastProfile[];
		busy?: boolean;
		onsave: (req: PlanRequest) => void;
		oncancel: () => void;
	} = $props();

	// svelte-ignore state_referenced_locally
	const p = plan;
	let deviceId = $state(p ? (p.device_id ?? '') : (roast.deviceId ?? ''));
	let beanId = $state(p?.bean_id ?? '');
	let profileId = $state(p?.profile_id ?? '');
	let weight = $state<number | null>(p?.green_weight_g ?? null);
	let name = $state(p?.name ?? '');
	let notes = $state(p?.notes ?? '');

	// Beans to pick: not archived, plus the plan's own bean if it was archived since.
	const choices = $derived(beans.filter((b) => !b.archived || b.id === beanId));

	function save(e: SubmitEvent) {
		e.preventDefault();
		onsave({
			device_id: deviceId || null,
			bean_id: beanId || null,
			profile_id: profileId || null,
			green_weight_g: weight,
			name: name.trim() || null,
			notes: notes.trim() || null
		});
	}

	const field =
		'w-full rounded-md border border-border bg-input px-2.5 py-1.5 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const label = 'mb-0.5 block text-xs font-medium text-muted-foreground';
</script>

<form onsubmit={save} class="grid grid-cols-2 gap-x-3 gap-y-2 sm:grid-cols-4">
	<div class="col-span-2">
		<label for="plan-bean" class={label}>Bean</label>
		<select id="plan-bean" bind:value={beanId} class={field}>
			<option value="">No bean from stock</option>
			{#each choices as b (b.id)}
				<option value={b.id}>{b.name} ({fmtGrams(b.stock_g)})</option>
			{/each}
		</select>
	</div>
	<div>
		<label for="plan-weight" class={label}>Green weight ({weightUnit()})</label>
		<WeightInput id="plan-weight" bind:grams={weight} class={field} />
	</div>
	<div>
		<label for="plan-roaster" class={label}>Roaster</label>
		<select id="plan-roaster" bind:value={deviceId} class={field}>
			<option value="">Any roaster</option>
			{#each roast.devices as d (d.device_id)}
				<option value={d.device_id}>{d.name}</option>
			{/each}
		</select>
	</div>
	<div class="col-span-2">
		<label for="plan-profile" class={label}>Profile</label>
		<select id="plan-profile" bind:value={profileId} class={field}>
			<option value="">No profile: record only</option>
			{#each profiles as pr (pr.id)}
				<option value={pr.id}>{pr.name}</option>
			{/each}
		</select>
	</div>
	<div class="col-span-2">
		<label for="plan-name" class={label}>Name (optional)</label>
		<input id="plan-name" bind:value={name} placeholder="Becomes the roast's name" class={field} />
	</div>
	<div class="col-span-2 sm:col-span-4">
		<label for="plan-notes" class={label}>Notes</label>
		<input id="plan-notes" bind:value={notes} class={field} />
	</div>
	<div class="col-span-2 flex gap-2 sm:col-span-4">
		<button
			type="submit"
			disabled={busy}
			class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
		>
			{busy ? 'Saving…' : p ? 'Save' : 'Add to plan'}
		</button>
		<button
			type="button"
			onclick={oncancel}
			class="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent"
			>Cancel</button
		>
	</div>
</form>
