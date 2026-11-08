<script lang="ts">
	import type { RoastEvent } from '#lib/api/events.js';
	import { fmtDelta, tempUnit } from '#lib/stores/units.svelte.js';
	import { phaseTimes } from '#lib/roast/phases.js';

	interface Props {
		events: RoastEvent[];
		totalTimeSeconds: number | null;
		avgRorDrying?: number | null;
		avgRorMaillard?: number | null;
		avgRorDevelopment?: number | null;
		/** No card or title: for a tab body that is already labelled. */
		flat?: boolean;
		/** A roast in progress started at this time: shares and DTR run to now. */
		liveStart?: string | null;
	}

	let {
		events,
		totalTimeSeconds,
		avgRorDrying = null,
		avgRorMaillard = null,
		avgRorDevelopment = null,
		flat = false,
		liveStart = null
	}: Props = $props();

	// The time since CHARGE while a roast runs, once a second.
	let nowMs = $state(Date.now());
	$effect(() => {
		if (!liveStart) return;
		const t = setInterval(() => (nowMs = Date.now()), 1000);
		return () => clearInterval(t);
	});
	const sinceCharge = $derived.by(() => {
		const charge = events.find((e) => e.event_type === 'charge');
		if (!liveStart || !charge) return null;
		return (nowMs - new Date(liveStart).getTime()) / 1000 - charge.elapsed_seconds;
	});
	const times = $derived(phaseTimes(events, totalTimeSeconds, sinceCharge));

	interface PhaseStats {
		name: string;
		color: string;
		duration: number | null;
		pct: number | null;
		avgRor: number | null;
		tempRise: number | null;
	}

	function formatDuration(secs: number): string {
		const m = Math.floor(secs / 60);
		const s = secs % 60;
		return `${m}:${s.toString().padStart(2, '0')}`;
	}

	let phases = $derived.by<PhaseStats[]>(() => {
		const find = (t: string) => events.find((e) => e.event_type === t);
		const charge = find('charge');
		const dryEnd = find('drying_end');
		const fcStart = find('first_crack_start');
		const drop = find('drop');
		const total = times.total ?? 0;

		const dryingDur = times.drying;
		const maillardDur = times.maillard;
		const devDur = times.development;

		const dryingTempRise =
			dryEnd?.temperature != null && charge?.temperature != null
				? dryEnd.temperature - charge.temperature
				: null;
		const maillardTempRise =
			fcStart?.temperature != null && dryEnd?.temperature != null
				? fcStart.temperature - dryEnd.temperature
				: null;
		const devTempRise =
			drop?.temperature != null && fcStart?.temperature != null
				? drop.temperature - fcStart.temperature
				: null;

		function computeRor(
			precomputed: number | null | undefined,
			tempRise: number | null,
			dur: number | null
		): number | null {
			if (precomputed != null) return Math.round(precomputed * 10) / 10;
			if (tempRise != null && dur != null && dur > 0) {
				return Math.round((tempRise / dur) * 60 * 10) / 10;
			}
			return null;
		}

		return [
			{
				name: 'Drying',
				color: '#22c55e',
				duration: dryingDur,
				pct: dryingDur != null && total ? (dryingDur / total) * 100 : null,
				avgRor: computeRor(avgRorDrying, dryingTempRise, dryingDur),
				tempRise: dryingTempRise != null ? Math.round(dryingTempRise * 10) / 10 : null
			},
			{
				name: 'Maillard',
				color: '#eab308',
				duration: maillardDur,
				pct: maillardDur != null && total ? (maillardDur / total) * 100 : null,
				avgRor: computeRor(avgRorMaillard, maillardTempRise, maillardDur),
				tempRise: maillardTempRise != null ? Math.round(maillardTempRise * 10) / 10 : null
			},
			{
				name: 'Development',
				color: '#92400e',
				duration: devDur,
				pct: devDur != null && total ? (devDur / total) * 100 : null,
				avgRor: computeRor(avgRorDevelopment, devTempRise, devDur),
				tempRise: devTempRise != null ? Math.round(devTempRise * 10) / 10 : null
			}
		];
	});

	let dtr = $derived(times.dtr);
</script>

<div class={flat ? '' : 'rounded-lg border border-border bg-card p-4'}>
	{#if !flat}<h3 class="mb-3 text-sm font-semibold text-foreground">Phases</h3>{/if}
	<table class="w-full text-sm">
		<thead>
			<tr class="border-b border-border">
				<th class="py-1 text-left font-medium text-muted-foreground">Phase</th>
				<th class="py-1 text-right font-medium text-muted-foreground">Duration</th>
				<th class="py-1 text-right font-medium text-muted-foreground">%</th>
				<th class="py-1 text-right font-medium text-muted-foreground">Avg RoR</th>
				<th class="py-1 text-right font-medium text-muted-foreground">&Delta;T</th>
			</tr>
		</thead>
		<tbody>
			{#each phases as phase (phase.name)}
				<tr class="border-b border-border/50">
					<td class="py-1.5">
						<span
							class="inline-block mr-1.5 h-2.5 w-2.5 rounded-full"
							style="background-color: {phase.color}"
						></span>
						<span class="text-foreground">{phase.name}</span>
					</td>
					<td class="py-1.5 text-right text-muted-foreground">
						{phase.duration != null ? formatDuration(Math.round(phase.duration)) : '—'}
					</td>
					<td class="py-1.5 text-right text-muted-foreground">
						{phase.pct != null ? `${phase.pct.toFixed(1)}%` : '—'}
					</td>
					<td class="py-1.5 text-right text-muted-foreground">
						{phase.avgRor != null ? `${fmtDelta(phase.avgRor)}${tempUnit()}/min` : '—'}
					</td>
					<td class="py-1.5 text-right text-muted-foreground">
						{phase.tempRise != null ? `${fmtDelta(phase.tempRise, 1, true)}${tempUnit()}` : '—'}
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
	{#if dtr != null}
		<div class="mt-3 flex items-center gap-2 border-t border-border pt-2">
			<span class="text-sm font-semibold text-foreground">DTR: {dtr}%</span>
			<span class="text-xs text-muted-foreground">(Development Time Ratio)</span>
		</div>
	{/if}
</div>
