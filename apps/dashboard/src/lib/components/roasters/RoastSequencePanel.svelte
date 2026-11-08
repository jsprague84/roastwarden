<script lang="ts">
	/**
	 * What CHARGE and DROP do to this roaster (docs/roast-sequence.md). The server runs it,
	 * so it works from any screen and for an auto-marked DROP.
	 */
	import { ListOrdered } from '@lucide/svelte';
	import { deviceId } from '#lib/stores/telemetry.js';
	import { roastSequence, type SequenceConfig } from '#lib/api/sequence.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { errorText } from '#lib/autotune/tuning.js';
	import { chargeLine, dropLine } from '#lib/roast/sequence.js';
	import {
		toDisplay,
		fromDisplay,
		roundForInput,
		tempUnit,
		fmtTemp
	} from '#lib/stores/units.svelte.js';

	/** The roaster's fan floor while heating (docs/roaster-setup.md); the fans keep to it. */
	let { fanFloor = 45 }: { fanFloor?: number } = $props();

	let saved = $state<SequenceConfig | null>(null);
	let draft = $state<SequenceConfig | null>(null);
	let coolDisplay = $state(0);
	let saving = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		const id = $deviceId;
		if (!id) return;
		roastSequence
			.get(id)
			.then((v) => {
				saved = v.config;
				draft = { ...v.config };
				coolDisplay =
					v.config.cool_end_bt_c > 0 ? roundForInput(toDisplay(v.config.cool_end_bt_c), 0) : 0;
			})
			.catch((e) => (error = errorText(e)));
	});

	function current(): SequenceConfig | null {
		if (!draft) return null;
		const cool =
			Number(coolDisplay) > 0 ? Math.round(fromDisplay(Number(coolDisplay)) * 10) / 10 : 0;
		return {
			...draft,
			charge_fan_pct: Number(draft.charge_fan_pct),
			charge_heater_pct: Number(draft.charge_heater_pct),
			cool_fan_pct: Number(draft.cool_fan_pct),
			cool_end_bt_c: cool
		};
	}
	const dirty = $derived.by(() => {
		void coolDisplay;
		const c = current();
		return !!c && !!saved && JSON.stringify(c) !== JSON.stringify(saved);
	});
	const preview = $derived.by(() => {
		void coolDisplay;
		const c = current();
		return [chargeLine(c, true), dropLine(c, (v) => `${fmtTemp(v, 0)}${tempUnit()}`)].filter(
			Boolean
		) as string[];
	});

	async function save() {
		const c = current();
		if (!$deviceId || !c) return;
		saving = true;
		error = null;
		try {
			const v = await roastSequence.put($deviceId, c);
			saved = v.config;
			draft = { ...v.config };
			notifications.add('Roast sequence saved', 'success');
		} catch (e) {
			error = errorText(e);
		} finally {
			saving = false;
		}
	}

	const num =
		'w-20 rounded-md border border-border bg-input px-2 py-1 text-sm tabular-nums text-foreground';
</script>

<section class="rounded-lg border border-border bg-card p-4" aria-labelledby="seq-title">
	<div class="flex items-center gap-2">
		<ListOrdered class="h-4 w-4 text-muted-foreground" />
		<h2 id="seq-title" class="text-sm font-semibold text-foreground">Roast sequence</h2>
	</div>
	<p class="mt-1 text-xs text-muted-foreground">
		What Charge and Drop do to this roaster, whoever marks them: a button, an alarm or the auto
		DROP. {#if fanFloor > 0}Fans stay at {fanFloor} % or more (the roaster's fan floor while heating).{/if}
	</p>

	{#if !draft}
		<p class="mt-3 text-xs text-muted-foreground">{error ?? 'Loading…'}</p>
	{:else}
		<div class="mt-3 space-y-3 text-sm">
			<fieldset class="space-y-2">
				<label class="flex items-center gap-2">
					<input type="checkbox" bind:checked={draft.charge_enabled} />
					<span class="font-medium text-foreground">At Charge: heater power on, fan to</span>
					<input
						type="number"
						min={fanFloor}
						max="100"
						step="1"
						bind:value={draft.charge_fan_pct}
						disabled={!draft.charge_enabled}
						class={num}
						aria-label="Fan at charge, %"
					/>
					<span class="text-muted-foreground">%</span>
				</label>
				<label class="ml-6 flex items-center gap-2">
					<input
						type="checkbox"
						bind:checked={draft.charge_drive}
						disabled={!draft.charge_enabled}
					/>
					<span class="text-foreground">and start Profile drive when the roast has a profile</span>
				</label>
				<label class="ml-6 flex flex-wrap items-center gap-2">
					<span class="text-foreground">with the heater starting at</span>
					<input
						type="number"
						min="0"
						max="100"
						step="5"
						bind:value={draft.charge_heater_pct}
						disabled={!draft.charge_enabled || !draft.charge_drive}
						class={num}
						aria-label="Heater the drive starts from, %"
					/>
					<span class="text-muted-foreground"
						>% (the roaster's PID takes over from there; 0 starts it from nothing, which held the
						heater low for the first minutes)</span
					>
				</label>
			</fieldset>
			<fieldset class="space-y-2">
				<label class="flex items-center gap-2">
					<input type="checkbox" bind:checked={draft.drop_enabled} />
					<span class="font-medium text-foreground"
						>At Drop: stop the drive, heater power off, cool at</span
					>
					<input
						type="number"
						min={fanFloor}
						max="100"
						step="1"
						bind:value={draft.cool_fan_pct}
						disabled={!draft.drop_enabled}
						class={num}
						aria-label="Cooling fan, %"
					/>
					<span class="text-muted-foreground">%</span>
				</label>
				<label class="ml-6 flex items-center gap-2">
					<span class="text-foreground">until BT and ET are below</span>
					<input
						type="number"
						min="0"
						step="1"
						bind:value={coolDisplay}
						disabled={!draft.drop_enabled}
						class={num}
						aria-label="Cooling ends below, {tempUnit()}"
					/>
					<span class="text-muted-foreground">{tempUnit()} (0: keep the fan running)</span>
				</label>
			</fieldset>
			{#if preview.length}
				<ul class="space-y-0.5 text-xs text-muted-foreground">
					{#each preview as l (l)}<li>{l}</li>{/each}
				</ul>
			{/if}
			{#if error}<p class="text-xs text-red-400">{error}</p>{/if}
			<div class="flex justify-end">
				<button
					onclick={save}
					disabled={!dirty || saving}
					class="rounded-md bg-action px-4 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-40"
				>
					{saving ? 'Saving…' : 'Save'}
				</button>
			</div>
		</div>
	{/if}
</section>
