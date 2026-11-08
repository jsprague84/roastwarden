<script lang="ts">
	/**
	 * A roaster's declared channels (docs/channels.md), in the Setup panel under Outputs: the
	 * list in a line, and an editor (add, edit, remove) whose keys come from what the roaster
	 * sends now (or are typed). Saved with the setup's `PUT`, owner key only.
	 */
	import { roasterSetup, type RoasterChannel, type RoasterSetup } from '#lib/api/roasters.js';
	import {
		CHANNELS_HELP,
		MAX_CHANNELS,
		candidateKeys,
		channelAxis,
		channelsLine,
		channelsProblem,
		newChannel,
		tidyChannels
	} from '#lib/roaster/channels.js';
	import Hint from '#lib/components/ui/Hint.svelte';

	let {
		setup,
		locked,
		onsave
	}: {
		setup: RoasterSetup;
		/** Not the owner, the fixed simulator, or a save running. */
		locked: boolean;
		onsave: (channels: RoasterChannel[]) => Promise<boolean>;
	} = $props();

	let editing = $state(false);
	let draft = $state<RoasterChannel[]>([]);
	let live = $state<string[]>([]);

	function edit() {
		draft = structuredClone($state.snapshot(setup.channels ?? []));
		editing = true;
		live = [];
		roasterSetup
			.latest(setup.device_id)
			.then((r) => (live = candidateKeys(r.telemetry)))
			.catch(() => (live = []));
	}
	function add(key = '') {
		if (draft.length < MAX_CHANNELS) draft.push(newChannel(key));
	}
	async function save() {
		if (await onsave(tidyChannels($state.snapshot(draft)))) editing = false;
	}
	const unused = $derived(live.filter((k) => !draft.some((c) => c.key === k)));
	const problem = $derived(editing ? channelsProblem(draft) : null);
	const field =
		'mt-0.5 w-full rounded-md border border-border bg-input px-2 py-1 text-sm text-foreground';
</script>

<div class="mt-4 border-t border-border pt-3">
	<div class="flex items-center justify-between gap-2">
		<span class="flex items-center gap-1 text-xs font-medium text-muted-foreground">
			Channels <Hint inline help={CHANNELS_HELP} />
		</span>
		{#if !editing && !locked}
			<button
				type="button"
				onclick={edit}
				class="text-xs font-medium text-foreground underline-offset-2 hover:underline">Edit</button
			>
		{/if}
	</div>
	{#if !editing}
		<p class="mt-1 text-sm text-foreground" data-testid="setup-channels">
			{channelsLine(setup.channels ?? [])}
		</p>
	{:else}
		<div class="mt-2 space-y-2">
			<datalist id="channel-keys-{setup.device_id}">
				{#each unused as k (k)}<option value={k}></option>{/each}
			</datalist>
			{#each draft as c, i (i)}
				<div class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_6rem_5rem_auto] items-end gap-2">
					<label class="text-[11px] text-muted-foreground"
						>Key<input
							class={field}
							bind:value={c.key}
							list="channel-keys-{setup.device_id}"
							maxlength="32"
							placeholder="t3"
						/></label
					>
					<label class="text-[11px] text-muted-foreground"
						>Label<input
							class={field}
							bind:value={c.label}
							maxlength="24"
							placeholder="Exhaust"
						/></label
					>
					<label class="text-[11px] text-muted-foreground"
						>Unit
						<select
							class={field}
							value={c.unit === 'C' || c.unit === '%' ? c.unit : 'other'}
							onchange={(e) =>
								(c.unit = e.currentTarget.value === 'other' ? '' : e.currentTarget.value)}
						>
							<option value="C">°C</option>
							<option value="%">%</option>
							<option value="other">other…</option>
						</select>
					</label>
					{#if channelAxis(c) === 'own'}
						<label class="text-[11px] text-muted-foreground"
							>Unit, top<span class="flex gap-1"
								><input class={field} bind:value={c.unit} maxlength="8" placeholder="kPa" /><input
									class={field}
									type="number"
									min="0"
									step="any"
									value={c.max ?? ''}
									oninput={(e) =>
										(c.max = e.currentTarget.value === '' ? null : Number(e.currentTarget.value))}
									placeholder="auto"
									title="Top of its scale; empty: the highest value seen"
								/></span
							></label
						>
					{:else}
						<span></span>
					{/if}
					<button
						type="button"
						onclick={() => draft.splice(i, 1)}
						class="pb-1 text-xs text-muted-foreground hover:text-foreground"
						aria-label="Remove {c.label || c.key}">Remove</button
					>
				</div>
			{/each}
			<div class="flex flex-wrap items-center gap-2 text-xs">
				{#if draft.length < MAX_CHANNELS}
					<button
						type="button"
						onclick={() => add()}
						class="font-medium text-foreground underline-offset-2 hover:underline"
						>Add a channel</button
					>
					{#each unused.slice(0, 6) as k (k)}
						<button
							type="button"
							onclick={() => add(k)}
							class="rounded border border-border px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
							title="The roaster sends {k} now: declare it">+ {k}</button
						>
					{/each}
				{/if}
				{#if live.length === 0}<span class="text-muted-foreground"
						>No extra values from this roaster right now: type the key.</span
					>{/if}
			</div>
			{#if problem}<p class="text-xs text-red-400">{problem}</p>{/if}
			<div class="flex gap-2">
				<button
					type="button"
					disabled={!!problem || locked}
					onclick={save}
					class="rounded-md bg-action px-3 py-1 text-xs font-medium text-action-foreground hover:bg-action-hover disabled:opacity-40"
					>Save channels</button
				>
				<button
					type="button"
					onclick={() => (editing = false)}
					class="rounded-md border border-border px-3 py-1 text-xs font-medium text-foreground hover:bg-accent"
					>Cancel</button
				>
			</div>
		</div>
	{/if}
</div>
