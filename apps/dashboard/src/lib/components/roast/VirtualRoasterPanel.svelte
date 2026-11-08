<script lang="ts">
	/**
	 * The simulators in the server (docs/virtual-roaster.md): the fluid bed `virtual_roaster`,
	 * modelled on this one, to rehearse profiles, alarms, replay and fan assist without beans;
	 * and the gas drum `virtual_drum` (docs/add-roaster.md), a stranger's roaster set up through
	 * an ingest mapping like any other. On the Roast page CHARGE and DROP load and dump the
	 * beans for you; the buttons here are for doing it by hand.
	 *
	 * `show`: both (Roasters), or only the one a page is about (its own page, the Roast page),
	 * so the Roast page's tab stays the size it was.
	 */
	import { FlaskConical } from '@lucide/svelte';
	import { virtualRoaster, type VirtualRoasterState } from '#lib/api/virtual.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { refreshDevices } from '#lib/stores/roast.svelte.js';
	import { fmtTemp, tempUnit } from '#lib/stores/units.svelte.js';
	import { HELP } from '#lib/constants/help.js';
	import { VIRTUAL_DEVICE_ID, VIRTUAL_DRUM_ID, isVisitorRoaster } from '#lib/roast/live.js';
	import Panel from '#lib/components/ui/Panel.svelte';
	import Hint from '#lib/components/ui/Hint.svelte';

	let {
		open = true,
		panelKey = 'simulator',
		show = 'both'
	}: { open?: boolean; panelKey?: string; show?: string | null } = $props();

	const fluid = $derived(show !== VIRTUAL_DRUM_ID);
	const drumShown = $derived(show === VIRTUAL_DRUM_ID || show === 'both' || show == null);
	/** A demo visitor's own simulated roaster: its state, beans and no on/off switch */
	const own = $derived(isVisitorRoaster(show) ? (show as string) : null);
	const sim = $derived(own ?? VIRTUAL_DEVICE_ID);

	let st = $state<VirtualRoasterState | null>(null);
	let busy = $state(false);

	async function refresh() {
		try {
			st = await virtualRoaster.get(own ?? undefined);
		} catch {
			/* server unreachable */
		}
	}

	$effect(() => {
		refresh();
		const t = setInterval(refresh, 2000);
		return () => clearInterval(t);
	});

	async function act(fn: () => Promise<VirtualRoasterState>, done: string) {
		busy = true;
		try {
			st = await fn();
			notifications.add(done, 'success');
			void refreshDevices();
		} catch (e) {
			notifications.add(`Simulator: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			busy = false;
		}
	}

	const drum = $derived(st?.drum ?? null);
	const btn =
		'rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-40';
</script>

<Panel title="Simulator" key={panelKey} {open}>
	{#snippet icon()}<FlaskConical class="h-4 w-4 text-emerald-400" />{/snippet}
	{#snippet summary()}
		{#if !st}…{:else if !fluid}{#if !drum?.enabled}drum off{:else}{drum.loaded
					? 'beans in the drum'
					: 'empty drum'} · BT {fmtTemp(
					drum.bt,
					0
				)}{tempUnit()}{/if}{:else if !st.enabled}off{#if drumShown && drum?.enabled}
				· drum on{/if}{:else}{st.loaded ? 'beans loaded' : 'empty chamber'} · BT {fmtTemp(
				st.bt,
				0
			)}{tempUnit()}{#if drumShown && drum?.enabled}
				· drum on{/if}{/if}
	{/snippet}

	{#if fluid}
		<div class="flex items-center justify-between gap-2">
			<Hint help={HELP.simulator} />
			{#if st && !own}
				<label class="flex shrink-0 items-center gap-2 text-sm text-foreground">
					<input
						type="checkbox"
						checked={st.enabled}
						disabled={busy}
						onchange={(e) =>
							act(
								() => virtualRoaster.setEnabled(e.currentTarget.checked),
								e.currentTarget.checked ? 'Virtual roaster on' : 'Virtual roaster off'
							)}
						class="accent-emerald-500"
					/>
					On
				</label>
			{/if}
		</div>
		{#if st?.enabled || (st && own)}
			<div class="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-sm sm:grid-cols-4">
				<span class="text-muted-foreground"
					>Chamber <span class="block font-medium text-foreground"
						>{st.loaded ? 'beans loaded' : 'empty'}</span
					></span
				>
				<span class="text-muted-foreground"
					>BT <span class="block font-semibold tabular-nums text-amber-400"
						>{fmtTemp(st.bt, 1)}{tempUnit()}</span
					></span
				>
				<span class="text-muted-foreground"
					>ET <span class="block font-semibold tabular-nums text-blue-400"
						>{fmtTemp(st.et, 1)}{tempUnit()}</span
					></span
				>
				<span class="text-muted-foreground"
					>Heat / fan <span class="block tabular-nums text-foreground"
						>{st.heater_applied.toFixed(0)} % · {st.fan.toFixed(0)} %</span
					></span
				>
			</div>
			<div class="mt-3 flex flex-wrap gap-2">
				<button
					class={btn}
					disabled={busy}
					onclick={() => act(() => virtualRoaster.charge(sim), 'Cold beans charged')}
					title="Load cold beans: the probe reads the beans, BT drops to room temperature"
					>Charge beans</button
				>
				<button
					class={btn}
					disabled={busy || !st.loaded}
					onclick={() => act(() => virtualRoaster.dump(sim), 'Beans dumped')}>Dump beans</button
				>
			</div>
			<p class="mt-2 text-[11px] text-muted-foreground">
				On the Roast page, Charge and Drop do this for you.
			</p>
		{:else if st}
			<p class="mt-2 text-xs text-muted-foreground">
				Off: switch it on, then pick <code>virtual_roaster</code> as the roaster.
			</p>
		{/if}
	{/if}

	{#if drumShown && st}
		<div class={fluid ? 'mt-4 border-t border-border pt-3' : ''}>
			<div class="flex items-center justify-between gap-2">
				<span class="flex min-w-0 items-center gap-1.5 text-sm font-medium text-foreground">
					Simulated drum <code class="text-xs text-muted-foreground">{VIRTUAL_DRUM_ID}</code>
					<Hint help={HELP.simulatedDrum} inline />
				</span>
				{#if drum}
					<label class="flex shrink-0 items-center gap-2 text-sm text-foreground">
						<input
							type="checkbox"
							checked={drum.enabled}
							disabled={busy}
							onchange={(e) =>
								act(
									() => virtualRoaster.setDrum(e.currentTarget.checked),
									e.currentTarget.checked ? 'Simulated drum on' : 'Simulated drum off'
								)}
							class="accent-emerald-500"
						/>
						On
					</label>
				{/if}
			</div>
			{#if !drum}
				<p class="mt-2 text-xs text-muted-foreground">This server has no simulated drum.</p>
			{:else if drum.enabled}
				<div class="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-sm sm:grid-cols-4">
					<span class="text-muted-foreground"
						>Drum <span class="block font-medium text-foreground"
							>{drum.loaded ? 'beans in' : 'empty'}</span
						></span
					>
					<span class="text-muted-foreground"
						>BT <span class="block font-semibold tabular-nums text-amber-400"
							>{fmtTemp(drum.bt, 1)}{tempUnit()}</span
						></span
					>
					<span class="text-muted-foreground"
						>Exhaust / inlet <span class="block tabular-nums text-blue-400"
							>{fmtTemp(drum.exhaust, 0)} · {fmtTemp(drum.inlet, 0)}{tempUnit()}</span
						></span
					>
					<span class="text-muted-foreground"
						>Burner / air / drum <span class="block tabular-nums text-foreground"
							>{drum.burner.toFixed(0)} · {drum.air.toFixed(0)} · {drum.drum.toFixed(0)} %</span
						></span
					>
				</div>
				<div class="mt-3 flex flex-wrap gap-2">
					<button
						class={btn}
						disabled={busy}
						onclick={() =>
							act(() => virtualRoaster.charge(VIRTUAL_DRUM_ID), 'Cold beans into the drum')}
						>Charge beans</button
					>
					<button
						class={btn}
						disabled={busy || !drum.loaded}
						onclick={() => act(() => virtualRoaster.dump(VIRTUAL_DRUM_ID), 'Drum emptied')}
						>Dump beans</button
					>
				</div>
			{:else}
				<p class="mt-2 text-xs text-muted-foreground">
					Off: switch it on, then pick <code>{VIRTUAL_DRUM_ID}</code> as the roaster.
				</p>
			{/if}
		</div>
	{/if}
</Panel>
