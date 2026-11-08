<script lang="ts">
	/**
	 * A server autotune run (docs/autotune-2.md): how far to trust it, what the roaster acts
	 * like near the test level, the record against the model, and the PI designs with what
	 * each costs. One knob from tight to smooth (SIMC's τc); apply to the roaster or the gain
	 * schedule at the run's fan.
	 */
	import { Check, CalendarClock } from '@lucide/svelte';
	import {
		tuning,
		type AutotuneRun,
		type Gains,
		type TuneDesign,
		type TuneSamples
	} from '#lib/api/tuning.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { fmtTemp, fmtDelta, tempUnit } from '#lib/stores/units.svelte.js';
	import {
		qualityInfo,
		toneBadge,
		toneText,
		fmtGains,
		errorText,
		tauCLabel,
		msInfo,
		toDeviceGains,
		describeModel
	} from '#lib/autotune/tuning.js';
	import { bump } from '#lib/autotune/bus.svelte.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { HELP } from '#lib/constants/help.js';
	import TuneFitChart from '#lib/components/roasters/TuneFitChart.svelte';

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

	const a = $derived(run.analysis);
	const q = $derived(qualityInfo(run.quality, run.state));
	const when = $derived(new Date(run.created_at).toLocaleString());
	const cur = $derived(fmtGains(current));

	let samples = $state<TuneSamples | null>(null);
	$effect(() => {
		const id = run.id;
		samples = null;
		tuning
			.samples(deviceId, id)
			.then((s) => (samples = s))
			.catch(() => (samples = null));
	});

	const RULES = [
		{ id: 'simc_pi', label: 'SIMC PI (recommended)' },
		{ id: 'amigo_pi', label: 'AMIGO PI (conservative)' },
		{ id: 'tyreus_luyben_pi', label: 'Tyreus–Luyben PI (for comparison)' },
		{ id: 'zn_pi', label: 'Ziegler–Nichols PI (for comparison)' }
	];
	let rule = $state('simc_pi');
	let tauC = $state(1.5);
	let preview = $state<{ design: TuneDesign; gains: Gains } | null>(null);
	let previewError = $state<string | null>(null);
	let busy = $state(false);

	// The design for the knob's position, from the server (one formula, there).
	$effect(() => {
		const [r, t, id] = [rule, tauC, run.id];
		if (!a?.design_model) return;
		const timer = setTimeout(() => {
			tuning
				.design(deviceId, id, r, t)
				.then((d) => {
					preview = d;
					previewError = null;
				})
				.catch((e) => {
					preview = null;
					previewError = errorText(e);
				});
		}, 150);
		return () => clearTimeout(timer);
	});

	const d = $derived(preview?.design ?? null);
	const g = $derived(preview ? fmtGains(preview.gains) : null);
	const ms = $derived(d ? msInfo(d.robustness.ms) : null);
	const canApply = $derived(run.usable && !!d && d.verdict !== 'refuse' && !busy);
	const num = (v: number | null | undefined, digits = 1) =>
		v == null || !Number.isFinite(v) ? '∞' : v.toFixed(digits);

	async function apply(to: 'device' | 'schedule') {
		busy = true;
		try {
			await tuning.applyRun(deviceId, run.id, rule, to, rule === 'simc_pi' ? tauC : undefined);
			const label = d?.label ?? rule;
			notifications.add(
				to === 'device'
					? `${label} stored on the roaster`
					: `${label} added to the schedule at ${run.fan_pct?.toFixed(0)} % fan`,
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

	function pick(design: TuneDesign) {
		const id =
			design.rule === 'simc' ? 'simc_pi' : design.rule === 'amigo' ? 'amigo_pi' : design.rule;
		rule = id;
		if (design.tau_c != null) tauC = design.tau_c;
	}
	const isPicked = (x: TuneDesign) =>
		(x.rule === 'simc' && rule === 'simc_pi' && Math.abs((x.tau_c ?? 0) - tauC) < 0.01) ||
		(x.rule === 'amigo' && rule === 'amigo_pi') ||
		(x.rule === rule && x.rule !== 'simc');
</script>

<div class="space-y-3 rounded-md border border-border bg-background p-3">
	<div class="flex flex-wrap items-center gap-2">
		<span class="rounded-full border px-2 py-0.5 text-xs font-medium {toneBadge[q.tone]}"
			>{q.label}</span
		>
		<span class="flex items-center gap-1 text-xs text-muted-foreground"
			><CalendarClock class="h-3 w-3" />{when}</span
		>
		{#if run.fan_pct != null}<span class="text-xs text-muted-foreground"
				>· fan {run.fan_pct.toFixed(0)} %</span
			>{/if}
		{#if run.test_temperature != null}
			<span class="text-xs text-muted-foreground"
				>· at {fmtTemp(run.test_temperature, 0)}{tempUnit()}</span
			>
		{/if}
		{#if run.bias != null}<span class="text-xs text-muted-foreground"
				>· holds with {run.bias.toFixed(0)} % heater</span
			>{/if}
	</div>

	{#if run.message && run.state !== 'complete'}
		<p class="text-xs text-red-400">The test {run.state}: {run.message}</p>
	{/if}
	{#if a}
		{#each a.warnings as w (w.code)}
			<p class="border-l-2 border-amber-500 pl-2 text-xs text-amber-400">{w.text}</p>
		{/each}

		{#if a.design_model}
			<p class="text-sm text-foreground">
				Near this level the roaster acts like {describeModel(a.design_model)}.
				<Hint inline help={HELP.tuneModel} />
			</p>
			<dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-xs sm:grid-cols-4">
				<div>
					<dt class="text-muted-foreground">Sensor noise</dt>
					<dd class="text-foreground">
						{a.noise != null ? `±${fmtDelta(a.noise, 2)}${tempUnit()}` : '—'}
					</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">Switching band</dt>
					<dd class="text-foreground">±{fmtDelta(a.hysteresis, 2)}{tempUnit()}</dd>
				</div>
				<div>
					<dt
						class="text-muted-foreground"
						title="Normalised dead time τ = L/(L+T): below 0.1 lag-dominant"
					>
						Dead time share
					</dt>
					<dd class="text-foreground">
						{a.tau != null ? a.tau.toFixed(2) : '—'}{a.berner_tau != null
							? ` (relay ${a.berner_tau.toFixed(2)})`
							: ''}
					</dd>
				</div>
				<div>
					<dt
						class="text-muted-foreground"
						title="The model's response at the oscillation against what the oscillation measured directly"
					>
						Model vs oscillation
					</dt>
					<dd class="text-foreground">
						{a.agreement
							? `×${a.agreement.gain_ratio.toFixed(2)}, ${a.agreement.phase_diff_deg >= 0 ? '+' : ''}${a.agreement.phase_diff_deg.toFixed(0)}°`
							: '—'}
					</dd>
				</div>
			</dl>
		{/if}

		{#if samples}
			<TuneFitChart {samples} />
		{/if}

		{#if a.design_model}
			<!-- The knob and the design it gives -->
			<div class="space-y-2 rounded-md border border-border p-2">
				<div class="flex flex-wrap items-center gap-3">
					<label class="text-xs text-foreground">
						Rule
						<select
							bind:value={rule}
							class="ml-1 rounded border border-border bg-input px-1.5 py-1 text-xs text-foreground"
						>
							{#each RULES as r (r.id)}<option value={r.id}>{r.label}</option>{/each}
						</select>
					</label>
					{#if rule === 'simc_pi'}
						<div
							class="flex w-full flex-wrap items-center gap-x-2 gap-y-1 text-xs text-foreground sm:w-auto sm:flex-1"
						>
							<span class="text-muted-foreground">Tight</span>
							<input
								type="range"
								min="1"
								max="3"
								step="0.1"
								bind:value={tauC}
								class="min-w-24 flex-1 accent-violet-500"
								aria-label="Tight to smooth"
							/>
							<span class="text-muted-foreground">Smooth</span>
							<span class="tabular-nums"
								>{tauC.toFixed(1)}·dead time, {tauCLabel(tauC)}
								<Hint inline help={HELP.tuneKnob} /></span
							>
						</div>
					{/if}
				</div>
				{#if previewError}
					<p class="text-xs text-red-400">{previewError}</p>
				{:else if d && g && ms}
					<div class="grid grid-cols-2 gap-x-4 gap-y-1 text-xs sm:grid-cols-4">
						<div>
							<span class="text-muted-foreground">Kp</span>
							<span class="font-medium tabular-nums text-foreground">{g.kp}</span>
						</div>
						<div>
							<span class="text-muted-foreground">Ki</span>
							<span class="font-medium tabular-nums text-foreground">{g.ki}</span>
						</div>
						<div>
							<span
								class="text-muted-foreground"
								title="Maximum sensitivity: 1.2–1.4 robust, up to 1.8 balanced, above 2 fragile"
								>Ms</span
							>
							<span class="font-medium tabular-nums {toneText[ms.tone]}"
								>{d.robustness.ms.toFixed(2)} {ms.label}</span
							>
						</div>
						<div>
							<span class="text-muted-foreground">Margins</span>
							<span class="tabular-nums text-foreground"
								>×{num(d.robustness.gain_margin)} · {num(d.robustness.phase_margin_deg, 0)}° · {num(
									d.robustness.delay_margin_s,
									0
								)} s</span
							>
						</div>
						<div>
							<span
								class="text-muted-foreground"
								title="How much the heater moves from the sensor's noise alone">Heater jitter</span
							>
							<span class="tabular-nums text-foreground">±{num(d.jitter)} %</span>
						</div>
						<div>
							<span
								class="text-muted-foreground"
								title="How far BT trails a 10 °C/min ramp with no lead; the drive's lookahead closes it"
								>Ramp lag</span
							>
							<span class="tabular-nums text-foreground"
								>{d.ramp_lag != null ? `${fmtDelta(d.ramp_lag, 1)}${tempUnit()}` : '—'}</span
							>
						</div>
						{#if d.robust_box}
							<div class="col-span-2">
								<span
									class="text-muted-foreground"
									title="Ms if the roaster is not what the test saw">If the roaster changes</span
								>
								<span class="tabular-nums text-foreground"
									>gain ×2: {d.robust_box.gain_x2.toFixed(1)} · dead time ×2: {d.robust_box.delay_x2.toFixed(
										1
									)}</span
								>
							</div>
						{/if}
					</div>
					{#each d.notes as n (n)}<p class="text-xs text-amber-400">{n}</p>{/each}
				{/if}
				{#if current}
					<p class="text-xs text-muted-foreground">
						In use now: Kp {cur.kp} · Ki {cur.ki} · Kd {cur.kd}
					</p>
				{/if}
				<div class="flex flex-wrap gap-2">
					<button
						onclick={() => apply('schedule')}
						disabled={!canApply || run.fan_pct == null}
						class="flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-xs font-medium text-action-foreground hover:bg-action-hover disabled:opacity-40"
					>
						<Check class="h-3.5 w-3.5" /> Add to schedule at {run.fan_pct?.toFixed(0) ?? '?'} % fan
					</button>
					<button
						onclick={() => apply('device')}
						disabled={!canApply}
						class="rounded-md border border-border px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent disabled:opacity-40"
					>
						Store on the roaster
					</button>
				</div>
				{#if !run.usable && run.unusable_reason}
					<p class="text-xs text-muted-foreground">Cannot apply: {run.unusable_reason}</p>
				{/if}
			</div>

			<!-- Every design at a glance -->
			<details class="text-xs">
				<summary class="cursor-pointer text-muted-foreground hover:text-foreground"
					>All designs</summary
				>
				<table class="mt-1 w-full tabular-nums">
					<thead>
						<tr class="text-muted-foreground">
							<th class="text-left font-medium">Design</th>
							<th class="text-right font-medium">Kp</th>
							<th class="text-right font-medium">Ki</th>
							<th class="text-right font-medium">Ms</th>
							<th class="text-right font-medium">Jitter</th>
							<th class="text-right font-medium">Ramp lag</th>
						</tr>
					</thead>
					<tbody>
						{#each a.designs as x, i (i)}
							{@const dg = fmtGains(toDeviceGains(x.pid))}
							<tr
								class="cursor-pointer border-t border-border hover:bg-accent {isPicked(x)
									? 'text-selected'
									: 'text-foreground'}"
								onclick={() => pick(x)}
							>
								<td class="py-0.5">{x.label}{a.recommended === i ? ' ★' : ''}</td>
								<td class="text-right">{dg.kp}</td>
								<td class="text-right">{dg.ki}</td>
								<td class="text-right {toneText[msInfo(x.robustness.ms).tone]}"
									>{x.robustness.ms.toFixed(2)}</td
								>
								<td class="text-right">±{num(x.jitter)} %</td>
								<td class="text-right"
									>{x.ramp_lag != null ? `${fmtDelta(x.ramp_lag, 1)}${tempUnit()}` : '—'}</td
								>
							</tr>
						{/each}
					</tbody>
				</table>
			</details>
		{/if}
	{:else}
		<p class="text-xs text-muted-foreground">No analysis stored for this run.</p>
	{/if}
</div>
