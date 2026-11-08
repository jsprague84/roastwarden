<script lang="ts">
	/**
	 * Fan assist settings (docs/fan-assist.md). The roaster is heat-limited
	 * at full airflow; the assist lowers the fan while the PID heater is
	 * pinned and gives it back when the heater has room.
	 */
	import { Wind } from '@lucide/svelte';
	import { deviceId } from '#lib/stores/telemetry.js';
	import type { FanAssistConfig } from '#lib/api/fanAssist.js';
	import { fanAssistState, watchFanAssist, saveFanAssist } from '#lib/stores/fanAssist.svelte.js';
	import { assistSummary, isTrimming } from '#lib/fanAssist/fanAssist.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { errorText } from '#lib/autotune/tuning.js';
	import { toDisplay, fromDisplay, roundForInput, tempUnit } from '#lib/stores/units.svelte.js';

	/** The roaster's fan floor while heating (docs/roaster-setup.md): the lowest fan keeps to it. */
	let { fanFloor = 45 }: { fanFloor?: number } = $props();

	let draft = $state<FanAssistConfig | null>(null);
	let armDisplay = $state(0);
	let saving = $state(false);
	let error = $state<string | null>(null);

	$effect(() => {
		const id = $deviceId;
		if (!id) return;
		return watchFanAssist(id);
	});

	const view = $derived(fanAssistState.deviceId === $deviceId ? fanAssistState.view : null);
	const saved = $derived(view?.config ?? null);
	const line = $derived(assistSummary(view));

	// Take the saved settings as the draft when they first arrive, and again
	// when they change elsewhere (the control panel switch) while the draft
	// is clean.
	let base = $state<string | null>(null);
	$effect(() => {
		if (!saved) return;
		const s = JSON.stringify(saved);
		if (draft === null || (base !== null && JSON.stringify(current()) === base && s !== base)) {
			draft = { ...saved };
			armDisplay = roundForInput(toDisplay(saved.arm_bt_c), 0);
			base = s;
		}
	});

	function current(): FanAssistConfig | null {
		if (!draft) return null;
		return { ...draft, arm_bt_c: Math.round(fromDisplay(armDisplay) * 10) / 10 };
	}

	const dirty = $derived.by(() => {
		void armDisplay;
		const c = current();
		return !!c && !!saved && JSON.stringify(c) !== JSON.stringify(saved);
	});

	async function save() {
		const c = current();
		if (!$deviceId || !c) return;
		saving = true;
		try {
			const v = await saveFanAssist($deviceId, c);
			draft = { ...v.config };
			armDisplay = roundForInput(toDisplay(v.config.arm_bt_c), 0);
			base = JSON.stringify(v.config);
			error = null;
			notifications.add(
				`Fan assist ${v.config.enabled ? 'on' : 'off'} · floor ${v.config.min_fan_pct} %`,
				'success'
			);
		} catch (e) {
			error = errorText(e);
		} finally {
			saving = false;
		}
	}

	function revert() {
		if (!saved) return;
		draft = { ...saved };
		armDisplay = roundForInput(toDisplay(saved.arm_bt_c), 0);
	}

	const input =
		'w-20 rounded border border-border bg-input px-2 py-1 text-right text-sm tabular-nums text-foreground';
</script>

<div class="rounded-lg border border-border bg-card p-4">
	<div class="mb-1 flex items-center justify-between gap-2">
		<div class="flex items-center gap-2">
			<Wind class="h-5 w-5 text-sky-400" />
			<h2 class="text-lg font-semibold text-foreground">Fan assist</h2>
		</div>
		{#if draft}
			<label class="flex items-center gap-2 text-sm text-foreground">
				<input type="checkbox" bind:checked={draft.enabled} class="accent-sky-500" />
				On
			</label>
		{/if}
	</div>
	<p class="mb-3 text-sm text-muted-foreground">
		When the PID heater stays pinned near 100 %, the server lowers the fan a little at a time so BT
		can keep climbing, and gives the air back once the heater has room. Your fan setting (or the
		profile's) is the most it will use; the floor is the least. Works with the roaster PID in auto,
		with or without a replay.
	</p>

	{#if !$deviceId}
		<p class="text-sm text-muted-foreground">No device connected.</p>
	{:else if !draft}
		<p class="text-xs text-muted-foreground">Loading…</p>
	{:else}
		<div class="grid grid-cols-1 gap-x-6 gap-y-2 text-sm sm:grid-cols-2">
			<label class="flex items-center justify-between gap-2">
				<span
					class="text-foreground"
					title="The assist never takes the fan below this. Keep it where the beans still move."
					>Minimum fan</span
				>
				<span class="flex items-center gap-1"
					><input
						type="number"
						min={fanFloor}
						max="100"
						step="1"
						bind:value={draft.min_fan_pct}
						class={input}
					/> %</span
				>
			</label>
			<label class="flex items-center justify-between gap-2">
				<span
					class="text-foreground"
					title="Below this BT the assist waits, so the pinned heater after CHARGE leaves the fan alone."
					>Start above BT</span
				>
				<span class="flex items-center gap-1"
					><input type="number" step="1" bind:value={armDisplay} class={input} /> {tempUnit()}</span
				>
			</label>
			<label class="flex items-center justify-between gap-2">
				<span class="text-foreground" title="Filtered heater output above this lowers the fan."
					>Lower fan when heater above</span
				>
				<span class="flex items-center gap-1"
					><input
						type="number"
						min="10"
						max="100"
						step="1"
						bind:value={draft.high_heater_pct}
						class={input}
					/> %</span
				>
			</label>
			<label class="flex items-center justify-between gap-2">
				<span class="text-foreground" title="Filtered heater output below this gives the fan back."
					>Give fan back when heater below</span
				>
				<span class="flex items-center gap-1"
					><input
						type="number"
						min="10"
						max="100"
						step="1"
						bind:value={draft.low_heater_pct}
						class={input}
					/> %</span
				>
			</label>
			<label class="flex items-center justify-between gap-2">
				<span class="text-foreground">Lower at most</span>
				<span class="flex items-center gap-1"
					><input
						type="number"
						min="0.5"
						max="60"
						step="0.5"
						bind:value={draft.max_down_pct_per_min}
						class={input}
					/> %/min</span
				>
			</label>
			<label class="flex items-center justify-between gap-2">
				<span class="text-foreground">Give back at most</span>
				<span class="flex items-center gap-1"
					><input
						type="number"
						min="0.5"
						max="60"
						step="0.5"
						bind:value={draft.max_up_pct_per_min}
						class={input}
					/> %/min</span
				>
			</label>
			<label class="flex items-center justify-between gap-2">
				<span
					class="text-foreground"
					title="How much faster the fan comes down for each °C BT is behind its target (beyond 1 °C). 0 = heater only."
					>Faster per °C behind</span
				>
				<span class="flex items-center gap-1"
					><input
						type="number"
						min="0"
						max="10"
						step="0.5"
						bind:value={draft.lag_gain_pct_per_min_per_c}
						class={input}
					/> %/min</span
				>
			</label>
		</div>

		{#if line}
			<p class="mt-3 text-xs {isTrimming(view) ? 'text-sky-400' : 'text-muted-foreground'}">
				{line}{view?.state.source === 'replay' ? ' (replay)' : ''}
			</p>
		{/if}
		{#if error || fanAssistState.error}
			<p class="mt-2 text-xs text-red-400">{error ?? fanAssistState.error}</p>
		{/if}

		<div class="mt-3 flex flex-wrap gap-2">
			<button
				onclick={save}
				disabled={!dirty || saving}
				class="rounded-md bg-sky-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-sky-700 disabled:opacity-40"
			>
				{saving ? 'Saving…' : 'Save fan assist'}
			</button>
			{#if dirty}
				<button
					onclick={revert}
					class="rounded-md px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground"
					>Revert</button
				>
			{/if}
		</div>
	{/if}
</div>
