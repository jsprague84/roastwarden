<script lang="ts">
	/**
	 * The roaster's control: its setup (kind, control level, outputs, docs/roaster-setup.md)
	 * first, then what the level and outputs allow: how its PID gets gains, the gain schedule,
	 * fan assist (both outputs), the roast sequence and token access. Gain schedule and fan
	 * assist are the tuning module's, token access the agents module's (docs/lean-home.md).
	 */
	import { HELP } from '#lib/constants/help.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import RoasterSetupPanel from '#lib/components/roasters/RoasterSetupPanel.svelte';
	import GainScheduleEditor from '#lib/components/roasters/GainScheduleEditor.svelte';
	import FanAssistPanel from '#lib/components/roasters/FanAssistPanel.svelte';
	import RoastSequencePanel from '#lib/components/roasters/RoastSequencePanel.svelte';
	import TokenAccessPanel from '#lib/components/roasters/TokenAccessPanel.svelte';
	import type { RoasterSetup } from '#lib/api/roasters.js';
	import { allows, needsLine } from '#lib/roaster/setup.js';
	import type { PageProps } from './$types';
	import { isOn } from '#lib/stores/modules.svelte.js';

	let { data }: PageProps = $props();

	let setup = $state<RoasterSetup | null>(null);
	const level = $derived(setup?.control ?? null);
	/** Lines for the panels the level hides, in their order. */
	const missing = $derived(
		level
			? (['gainSchedule', 'fanAssist', 'sequence', 'tokenAccess'] as const)
					.filter((f) => f === 'sequence' || isOn(f === 'tokenAccess' ? 'agents' : 'tuning'))
					.map((f) => needsLine(f, level))
					.filter((l): l is string => l !== null)
			: []
	);
	/** Fan assist trims the fan against the heater: both outputs. */
	const bothOutputs = $derived(!!setup?.outputs.heater && !!setup?.outputs.fan);
	const floor = $derived(setup?.fan_floor_pct ?? 45);
</script>

<div class="space-y-6">
	<RoasterSetupPanel deviceId={data.device.device_id} bind:setup />
	{#if level}
		{#if allows(level, 'gainSchedule') && isOn('tuning')}
			<div class="space-y-1 rounded-lg border border-border bg-card p-4">
				<h2 class="text-sm font-semibold text-foreground">How this roaster's PID gets its gains</h2>
				<Hint help={HELP.gainSchedule} />
				<p class="text-xs text-muted-foreground">
					Stored gains are the last ones applied (by hand on the Roast page, from an autotune, or by
					an undo): see Autotune → Tuning history.
				</p>
			</div>
		{/if}
		{#if missing.length > 0}
			<div class="space-y-1 text-xs text-muted-foreground">
				{#each missing as line (line)}<p>{line}</p>{/each}
			</div>
		{/if}
		{#if allows(level, 'fanAssist') && !bothOutputs && isOn('tuning')}
			<p class="text-xs text-muted-foreground">Fan assist needs both a heater and a fan output.</p>
		{/if}
		{#if allows(level, 'sequence')}
			<!-- A kind change resets the sequence and fan assist: show the new ones. -->
			{#key setup?.kind}
				<div class="grid gap-6 xl:grid-cols-2">
					{#if isOn('tuning') && allows(level, 'gainSchedule') && setup?.outputs.heater}<GainScheduleEditor
						/>{/if}
					{#if isOn('tuning') && allows(level, 'fanAssist') && bothOutputs}<FanAssistPanel
							fanFloor={floor}
						/>{/if}
					<RoastSequencePanel fanFloor={floor} />
					{#if isOn('agents')}<TokenAccessPanel deviceId={data.device.device_id} />{/if}
				</div>
			{/key}
		{/if}
	{/if}
</div>
