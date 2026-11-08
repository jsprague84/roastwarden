<script lang="ts">
	/**
	 * One autotune run: quality and warnings, what the test measured, the
	 * gains every tuning rule gives next to the gains in use, and applying a
	 * rule to the roaster (stored) or to the gain schedule at the run's fan.
	 */
	import { Check, CalendarClock } from '@lucide/svelte';
	import { tuning, type AutotuneRun, type Gains } from '#lib/api/tuning.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { fmtTemp, fmtDelta, tempUnit } from '#lib/stores/units.svelte.js';
	import {
		qualityInfo,
		toneBadge,
		warningText,
		fmtGains,
		errorText
	} from '#lib/autotune/tuning.js';
	import { bump } from '#lib/autotune/bus.svelte.js';

	let {
		run,
		deviceId,
		current = null,
		onapplied
	}: {
		run: AutotuneRun;
		deviceId: string;
		current?: Gains | null;
		onapplied?: (to: 'device' | 'schedule') => void;
	} = $props();

	const q = $derived(qualityInfo(run.quality, run.state));
	const defaultRule = $derived(
		run.rules.some((r) => r.rule === run.tuning_method) ? run.tuning_method! : 'tyreus_luyben_pi'
	);
	let picked = $state<string | null>(null);
	const rule = $derived(picked ?? defaultRule);
	let busy = $state(false);

	const amp = $derived(
		typeof run.payload.oscillation_amplitude === 'number' ? run.payload.oscillation_amplitude : null
	);
	const swing = $derived(typeof run.payload.amplitude === 'number' ? run.payload.amplitude : null);
	const cycles = $derived(typeof run.payload.cycles === 'number' ? run.payload.cycles : null);
	const heatingS = $derived(
		typeof run.payload.heating_seconds === 'number' ? run.payload.heating_seconds : null
	);
	const when = $derived(new Date(run.created_at).toLocaleString());
	const cur = $derived(fmtGains(current));

	async function apply(to: 'device' | 'schedule') {
		busy = true;
		try {
			await tuning.applyRun(deviceId, run.id, rule, to);
			const label = run.rules.find((r) => r.rule === rule)?.label ?? rule;
			notifications.add(
				to === 'device'
					? `${label} gains stored on the roaster`
					: `${label} gains added to the schedule at ${run.fan_pct?.toFixed(0)} % fan`,
				'success'
			);
			bump('pid', 'schedule');
			onapplied?.(to);
		} catch (e) {
			notifications.add(`Could not apply: ${errorText(e)}`, 'error');
		} finally {
			busy = false;
		}
	}
</script>

<div class="space-y-3 rounded-md border border-border bg-background p-3">
	<div class="flex flex-wrap items-center gap-2">
		<span class="rounded-full border px-2 py-0.5 text-xs font-medium {toneBadge[q.tone]}"
			>{q.label}</span
		>
		<span class="flex items-center gap-1 text-xs text-muted-foreground">
			<CalendarClock class="h-3 w-3" />{when}
		</span>
		{#if run.fan_pct != null}
			<span class="text-xs text-muted-foreground">· fan {run.fan_pct.toFixed(0)} %</span>
		{/if}
		{#if run.test_temperature != null}
			<span class="text-xs text-muted-foreground">
				· at {fmtTemp(
					run.test_temperature,
					0
				)}{tempUnit()}{#if run.target_temperature != null && Math.abs(run.target_temperature - run.test_temperature) > 0.5}
					(target {fmtTemp(run.target_temperature, 0)}{tempUnit()}){/if}
			</span>
		{/if}
	</div>

	{#if !run.usable}
		<p class="rounded-md border border-red-500/30 bg-red-500/10 px-2 py-1 text-xs text-red-400">
			{run.unusable_reason ?? run.message ?? 'This run cannot be applied.'}
		</p>
	{/if}

	{#if run.warnings.length}
		<ul class="list-disc space-y-0.5 pl-5 text-xs text-amber-400">
			{#each run.warnings as w (w)}<li>{warningText(w)}</li>{/each}
		</ul>
	{/if}

	<dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-xs sm:grid-cols-3">
		<div>
			<dt class="text-muted-foreground" title="Ultimate gain, device PID units (0..255 per °C)">
				Ultimate gain Ku
			</dt>
			<dd class="font-medium text-foreground">{run.ku != null ? run.ku.toFixed(2) : '—'}</dd>
		</div>
		<div>
			<dt class="text-muted-foreground" title="Ultimate period">Period Tu</dt>
			<dd class="font-medium text-foreground">{run.tu != null ? `${run.tu.toFixed(0)} s` : '—'}</dd>
		</div>
		<div>
			<dt
				class="text-muted-foreground"
				title="Heater % that holds the test temperature at this fan"
			>
				Holding power
			</dt>
			<dd class="font-medium text-foreground">
				{run.bias != null ? `${run.bias.toFixed(0)} %` : '—'}
			</dd>
		</div>
		<div>
			<dt class="text-muted-foreground">BT swing</dt>
			<dd class="font-medium text-foreground">
				{amp != null ? `±${fmtDelta(amp, 1)}${tempUnit()}` : '—'}
			</dd>
		</div>
		<div>
			<dt class="text-muted-foreground">Relay swing</dt>
			<dd class="font-medium text-foreground">{swing != null ? `±${swing.toFixed(0)} %` : '—'}</dd>
		</div>
		<div>
			<dt class="text-muted-foreground">Cycles · heat-up</dt>
			<dd class="font-medium text-foreground">
				{cycles ?? '—'} · {heatingS != null ? `${Math.round(heatingS / 60)} min` : '—'}
			</dd>
		</div>
	</dl>

	{#if run.rules.length}
		<div class="overflow-x-auto">
			<table class="w-full text-xs">
				<thead>
					<tr class="text-muted-foreground">
						<th class="pb-1 text-left font-medium">Rule</th>
						<th class="pb-1 pl-3 text-right font-medium">Kp</th>
						<th class="pb-1 pl-3 text-right font-medium">Ki</th>
						<th class="pb-1 pl-3 text-right font-medium">Kd</th>
					</tr>
				</thead>
				<tbody>
					<tr class="text-muted-foreground">
						<td class="py-1">In use now</td>
						<td class="py-1 pl-3 text-right tabular-nums">{cur.kp}</td>
						<td class="py-1 pl-3 text-right tabular-nums">{cur.ki}</td>
						<td class="py-1 pl-3 text-right tabular-nums">{cur.kd}</td>
					</tr>
					{#each run.rules as r (r.rule)}
						{@const g = fmtGains(r)}
						<tr class="border-t border-border {rule === r.rule ? 'bg-amber-500/10' : ''}">
							<td class="py-1">
								<label class="flex cursor-pointer items-center gap-1.5 text-foreground">
									<input
										type="radio"
										name="rule-{run.id}"
										value={r.rule}
										checked={rule === r.rule}
										onchange={() => (picked = r.rule)}
										disabled={!run.usable}
										class="accent-amber-500"
									/>
									{r.label}
									{#if r.rule === run.tuning_method}<span class="text-muted-foreground">(run)</span
										>{/if}
									{#if r.clamped}<span class="text-amber-400" title="A gain hit its limit"
											>clamped</span
										>{/if}
								</label>
							</td>
							<td class="py-1 pl-3 text-right tabular-nums text-foreground">{g.kp}</td>
							<td class="py-1 pl-3 text-right tabular-nums text-foreground">{g.ki}</td>
							<td class="py-1 pl-3 text-right tabular-nums text-foreground">{g.kd}</td>
						</tr>
					{/each}
				</tbody>
			</table>
			<p class="mt-1 text-[11px] text-muted-foreground">
				PI rules leave out D, which mostly amplifies noise with 1 Hz thermocouple readings.
			</p>
		</div>

		{#if run.usable}
			<div class="flex flex-wrap gap-2">
				<button
					onclick={() => apply('device')}
					disabled={busy}
					class="flex flex-1 items-center justify-center gap-1.5 rounded-md bg-go px-3 py-1.5 text-xs font-medium text-go-foreground hover:bg-go-hover disabled:opacity-50"
				>
					<Check class="h-3.5 w-3.5" /> Store on roaster
				</button>
				<button
					onclick={() => apply('schedule')}
					disabled={busy || run.fan_pct == null}
					title={run.fan_pct == null
						? 'The run did not report its fan speed'
						: 'Use these gains whenever the fan is near this speed'}
					class="flex flex-1 items-center justify-center gap-1.5 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-1.5 text-xs font-medium text-amber-400 hover:bg-amber-500/20 disabled:opacity-50"
				>
					Add to schedule{run.fan_pct != null ? ` at ${run.fan_pct.toFixed(0)} % fan` : ''}
				</button>
			</div>
		{/if}
	{/if}
</div>
