<script lang="ts">
	/**
	 * The roast cockpit (docs/ui-structure.md): readouts and the scope on
	 * the left, the next step and the controls on the right (below on a
	 * phone, with the primary action pinned to the bottom). On a laptop it
	 * fits the window: nothing scrolls, the controls are tabs.
	 */
	import LiveScope from '#lib/components/roast/LiveScope.svelte';
	import RoastReadouts from '#lib/components/roast/RoastReadouts.svelte';
	import RoastStepCard from '#lib/components/roast/RoastStepCard.svelte';
	import RoastActionBar from '#lib/components/roast/RoastActionBar.svelte';
	import ReplayPanel from '#lib/components/roast/ReplayPanel.svelte';
	import RoasterControlPanel from '#lib/components/roast/RoasterControlPanel.svelte';
	import VirtualRoasterPanel from '#lib/components/roast/VirtualRoasterPanel.svelte';
	import PhaseStatsPanel from '#lib/components/roast/PhaseStatsPanel.svelte';
	import LiveAuc from '#lib/components/roast/LiveAuc.svelte';
	import AutoEventDetector from '#lib/components/roast/AutoEventDetector.svelte';
	import KeepAwake from '#lib/components/roast/KeepAwake.svelte';
	import AlarmMonitor from '#lib/components/roast/AlarmMonitor.svelte';
	import FlatPanels from '#lib/components/ui/FlatPanels.svelte';
	import { replayState } from '#lib/stores/telemetry.js';
	import { driveTabLabel } from '#lib/replay/replay.js';
	import { roast, refreshEvents } from '#lib/stores/roast.svelte.js';
	import { watchFanAssist } from '#lib/stores/fanAssist.svelte.js';
	import { isVirtual } from '#lib/roast/live.js';

	const activeSession = $derived(roast.session);
	const landmarks = $derived(
		roast.events.map((e) => ({
			type: e.event_type,
			elapsed_seconds: e.elapsed_seconds,
			temperature: e.temperature ?? undefined
		}))
	);
	const virtual = $derived(isVirtual(roast.deviceId));

	// Fan assist state feeds the readouts and the control panel.
	$effect(() => {
		const id = roast.deviceId;
		if (!id) return;
		return watchFanAssist(id);
	});

	// ---- Control tabs under the next step (kept per browser) ----
	type RailTab = 'control' | 'drive' | 'sim' | 'phases';
	const RAIL_KEY = 'roastwarden.rail';
	const showPhases = $derived(!!activeSession && roast.events.length > 0);
	const railTabs = $derived<{ id: RailTab; label: string }[]>([
		{ id: 'control', label: 'Control' },
		{ id: 'drive', label: 'Profile drive' },
		...(virtual ? [{ id: 'sim' as RailTab, label: 'Simulator' }] : []),
		...(showPhases ? [{ id: 'phases' as RailTab, label: 'Phases' }] : [])
	]);
	function loadRail(): RailTab {
		try {
			return (localStorage.getItem(RAIL_KEY) as RailTab) || 'control';
		} catch {
			return 'control';
		}
	}
	let railPick = $state<RailTab>(loadRail());
	const railTab = $derived(railTabs.some((t) => t.id === railPick) ? railPick : 'control');
	function pickRail(id: RailTab) {
		railPick = id;
		try {
			localStorage.setItem(RAIL_KEY, id);
		} catch {
			/* storage unavailable */
		}
	}
	// While it runs, Profile drive's tab shows its state and the profile time.
	const driveTab = $derived(driveTabLabel($replayState, activeSession?.id));
	const DRIVE_TONE = {
		live: 'text-green-400',
		ghost: 'text-sky-400',
		paused: 'text-amber-400'
	} as const;
</script>

<svelte:head>
	<title>Roast | Roastwarden</title>
</svelte:head>

<h1 class="sr-only">Roast</h1>

<div
	class="flex flex-col gap-3 pb-24 lg:grid lg:h-[calc(100dvh-3.5rem-1px-2rem)] lg:grid-cols-[minmax(0,1fr)_22rem] lg:pb-0 xl:grid-cols-[minmax(0,1fr)_25rem]"
>
	<div class="flex min-h-0 min-w-0 flex-col gap-3">
		<RoastReadouts />

		<div class="h-[58vh] min-h-[340px] lg:h-auto lg:min-h-0 lg:flex-1">
			<LiveScope {landmarks} {activeSession} liveReadout={false} />
		</div>
	</div>

	<aside
		class="flex min-h-0 min-w-0 flex-col gap-3 [@media(max-height:760px)]:gap-2"
		aria-label="Roast controls"
	>
		<div class="shrink-0"><RoastStepCard /></div>

		<section class="flex min-h-0 flex-col rounded-lg border border-border bg-card lg:flex-1">
			<div
				class="flex shrink-0 border-b border-border text-sm"
				role="tablist"
				aria-label="Controls"
			>
				{#each railTabs as t (t.id)}
					<button
						type="button"
						role="tab"
						aria-selected={railTab === t.id}
						aria-label={t.id === 'drive' && driveTab.status
							? `Profile drive: ${driveTab.status}`
							: undefined}
						onclick={() => pickRail(t.id)}
						class="-mb-px flex items-center gap-1.5 whitespace-nowrap border-b-2 px-3 py-2 font-medium transition-colors
						{railTab === t.id
							? 'border-action text-foreground'
							: 'border-transparent text-muted-foreground hover:text-foreground'}"
					>
						{#if t.id === 'drive'}
							{driveTab.label}
							{#if driveTab.status && driveTab.tone}<span
									class="text-xs tabular-nums {DRIVE_TONE[driveTab.tone]}">{driveTab.status}</span
								>{/if}
						{:else}
							{t.label}
						{/if}
					</button>
				{/each}
			</div>
			<div
				class="scroll-thin min-h-0 flex-1 overflow-y-auto p-3 [@media(max-height:760px)]:py-2"
				role="tabpanel"
			>
				<FlatPanels>
					{#if railTab === 'control'}
						<RoasterControlPanel />
					{:else if railTab === 'drive'}
						<ReplayPanel {activeSession} deviceId={roast.deviceId} />
					{:else if railTab === 'sim'}
						<VirtualRoasterPanel show={roast.deviceId} />
					{:else if railTab === 'phases' && activeSession}
						<PhaseStatsPanel
							events={roast.events}
							totalTimeSeconds={activeSession.total_time_seconds ?? null}
							liveStart={activeSession.start_time ?? null}
							flat
						/>
						<LiveAuc />
					{/if}
				</FlatPanels>
			</div>
		</section>
	</aside>
</div>

<RoastActionBar />

{#if !activeSession || roast.eventsLoaded}
	<AutoEventDetector {activeSession} events={roast.events} onmarked={refreshEvents} />
	<KeepAwake on={activeSession?.status === 'active' || activeSession?.status === 'paused'} />
{/if}
<AlarmMonitor {activeSession} sessionEvents={roast.events} />
