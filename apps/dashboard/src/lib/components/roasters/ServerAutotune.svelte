<script lang="ts">
	/**
	 * The server's autotune (docs/autotune-2.md): settings in plain terms, the test live, and
	 * its result. The server drives the roaster and keeps going if this page closes; opening
	 * it again picks the test up.
	 */
	import { Play, Square } from '@lucide/svelte';
	import { deviceId, telemetry } from '#lib/stores/telemetry.js';
	import { tuning, type AutotuneRun, type Gains, type TuneState } from '#lib/api/tuning.js';
	import { notifications } from '#lib/stores/notifications.js';
	import {
		toDisplay,
		fromDisplay,
		deltaToDisplay,
		deltaFromDisplay,
		fmtTemp,
		fmtDelta,
		tempUnit,
		roundForInput
	} from '#lib/stores/units.svelte.js';
	import { tunePhaseInfo, toneText, errorText } from '#lib/autotune/tuning.js';
	import { bump } from '#lib/autotune/bus.svelte.js';
	import { HELP } from '#lib/constants/help.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import AutotuneChart from '#lib/components/roasters/AutotuneChart.svelte';
	import TuneResult from '#lib/components/roasters/TuneResult.svelte';

	let {
		hasActiveSession = false,
		current = null
	}: { hasActiveSession?: boolean; current?: Gains | null } = $props();

	// Settings (temperatures in °C internally).
	let level = $state(190);
	let fanPercent = $state(100);
	let approachPower = $state(100);
	let maxSwing = $state(30);
	let asymmetry = $state(2);
	let autoBand = $state(true);
	let band = $state(0.5);
	let showAdvanced = $state(false);

	let st = $state<TuneState | null>(null);
	let result = $state<AutotuneRun | null>(null);
	let loading = $state(false);

	// The fan starts where the roaster is (once), not at 100 %.
	let fanSeeded = false;
	$effect(() => {
		const f = $telemetry?.fan;
		if (!fanSeeded && f != null && f >= 45) {
			fanPercent = Math.round(f);
			fanSeeded = true;
		}
	});

	const running = $derived(!!st?.active);
	const phase = $derived(tunePhaseInfo(st?.phase));
	const fanNow = $derived($telemetry?.fan != null ? Math.round($telemetry.fan) : null);
	const chartFrom = $derived(st ? Date.parse(st.started_at) : Date.now() - 20 * 60_000);

	// The test's state: often while it runs, once otherwise (to pick up one already running).
	$effect(() => {
		const id = $deviceId;
		if (!id) return;
		let stop = false;
		let timer: ReturnType<typeof setTimeout>;
		const poll = async () => {
			try {
				st = await tuning.tuneState(id);
			} catch {
				st = null;
			}
			if (stop) return;
			timer = setTimeout(poll, st?.active || st?.phase === 'analysing' ? 1000 : 10_000);
		};
		poll();
		return () => {
			stop = true;
			clearTimeout(timer);
		};
	});

	// Its result, once stored.
	let shownResult: string | null = null;
	$effect(() => {
		const id = $deviceId;
		const runId = st?.result_run_id;
		if (!id || !runId || runId === shownResult) return;
		shownResult = runId;
		tuning
			.run(id, runId)
			.then((r) => {
				result = r;
				bump('runs');
			})
			.catch(() => (result = null));
	});

	async function start() {
		if (!$deviceId) return;
		loading = true;
		result = null;
		try {
			st = await tuning.tuneStart($deviceId, {
				setpoint: Math.round(level * 10) / 10,
				fan_pct: fanPercent,
				approach_power: approachPower,
				max_swing: maxSwing,
				asymmetry,
				...(autoBand ? {} : { hysteresis: Math.round(band * 100) / 100 })
			});
		} catch (e) {
			notifications.add(`Could not start the autotune: ${errorText(e)}`, 'error');
		} finally {
			loading = false;
		}
	}

	async function stop() {
		if (!$deviceId) return;
		loading = true;
		try {
			st = await tuning.tuneStop($deviceId);
		} catch (e) {
			notifications.add(`Could not stop the autotune: ${errorText(e)}`, 'error');
		} finally {
			loading = false;
		}
	}

	const inputCls =
		'w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus disabled:opacity-50';
</script>

<div class="space-y-4">
	<Hint help={HELP.tuneServer} />
	{#if hasActiveSession && !running}
		<div
			class="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-sm text-amber-400"
		>
			A roast session is in progress. Complete or stop it before running autotune.
		</div>
	{:else}
		<div class="grid gap-3 sm:grid-cols-2">
			<div>
				<label for="tu-level" class="mb-1 block text-sm font-medium text-foreground"
					>Test at bean temperature ({tempUnit()})</label
				>
				<input
					id="tu-level"
					type="number"
					min={Math.round(toDisplay(100))}
					max={Math.round(toDisplay(250))}
					step={5}
					value={roundForInput(toDisplay(level), 0)}
					oninput={(e) => {
						const v = parseFloat(e.currentTarget.value);
						if (!Number.isNaN(v)) level = fromDisplay(v);
					}}
					disabled={running}
					class={inputCls}
				/>
				<p class="mt-1 text-xs text-muted-foreground">
					Where this fan is used in a roast: about {fmtTemp(160, 0)}{tempUnit()} at 100 % fan, {fmtTemp(
						190,
						0
					)}{tempUnit()} at 70–75 %. Below first crack.
				</p>
			</div>
			<div>
				<label for="tu-fan" class="mb-1 block text-sm font-medium text-foreground"
					>Fan ({fanPercent} %)</label
				>
				<input
					id="tu-fan"
					type="range"
					min="45"
					max="100"
					step="1"
					bind:value={fanPercent}
					disabled={running}
					class="mt-2 w-full accent-violet-500"
				/>
				<p class="mt-1 text-xs text-muted-foreground">
					Set before the test starts.{#if fanNow != null}{' '}Now {fanNow} %.{/if}
				</p>
			</div>
		</div>

		<div>
			<button
				type="button"
				onclick={() => (showAdvanced = !showAdvanced)}
				disabled={running}
				class="text-xs text-muted-foreground hover:text-foreground disabled:opacity-50"
			>
				{showAdvanced ? '▾' : '▸'} Advanced
			</button>
			{#if showAdvanced}
				<div
					class="mt-2 grid gap-3 rounded-md border border-border bg-background p-3 sm:grid-cols-2"
				>
					<div>
						<label for="tu-heat" class="mb-1 block text-xs font-medium text-foreground"
							>Heat-up power ({approachPower} %)</label
						>
						<input
							id="tu-heat"
							type="range"
							min="50"
							max="100"
							step="5"
							bind:value={approachPower}
							disabled={running}
							class="w-full accent-red-500"
						/>
					</div>
					<div>
						<label for="tu-swing" class="mb-1 block text-xs font-medium text-foreground"
							>Largest heater swing ({maxSwing} %)</label
						>
						<input
							id="tu-swing"
							type="range"
							min="5"
							max="50"
							step="1"
							bind:value={maxSwing}
							disabled={running}
							class="w-full accent-amber-500"
						/>
						<p class="text-xs text-muted-foreground">
							The test swings as wide as the room around the holding power allows, up to this.
						</p>
					</div>
					<div>
						<label for="tu-asym" class="mb-1 block text-xs font-medium text-foreground"
							>Asymmetry ({asymmetry.toFixed(1)}×)</label
						>
						<input
							id="tu-asym"
							type="range"
							min="1"
							max="4"
							step="0.5"
							bind:value={asymmetry}
							disabled={running}
							class="w-full accent-amber-500"
						/>
						<p class="text-xs text-muted-foreground">
							The larger swing over the smaller in the last phase (2 is standard; 1 skips the
							phase).
						</p>
					</div>
					<div>
						<label class="mb-1 flex items-center gap-2 text-xs font-medium text-foreground">
							<input
								type="checkbox"
								bind:checked={autoBand}
								disabled={running}
								class="accent-violet-500"
							/>
							Switching band from the sensor's noise
						</label>
						{#if !autoBand}
							<input
								type="number"
								min={roundForInput(deltaToDisplay(0.1), 2)}
								max={roundForInput(deltaToDisplay(5), 1)}
								step={0.1}
								value={roundForInput(deltaToDisplay(band), 2)}
								oninput={(e) => {
									const v = parseFloat(e.currentTarget.value);
									if (!Number.isNaN(v)) band = deltaFromDisplay(v);
								}}
								disabled={running}
								aria-label="Switching band"
								class="w-full rounded-md border border-border bg-input px-2 py-1.5 text-xs text-foreground disabled:opacity-50"
							/>
						{/if}
					</div>
				</div>
			{/if}
		</div>
	{/if}

	{#if st && (running || st.phase === 'analysing' || st.phase === 'failed')}
		<div class="space-y-2 rounded-md border border-border bg-background p-3">
			<div class="flex flex-wrap items-center justify-between gap-2">
				<span class="text-sm font-medium {toneText[phase.tone]}">{phase.label}</span>
				<span class="text-xs text-muted-foreground"
					>{Math.floor(st.elapsed_s / 60)}:{String(Math.floor(st.elapsed_s % 60)).padStart(2, '0')} ·
					{st.cycles} cycles</span
				>
			</div>
			<dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-xs sm:grid-cols-4">
				<div>
					<dt class="text-muted-foreground">BT · level</dt>
					<dd class="font-medium text-foreground">
						{fmtTemp(st.bt)} · {fmtTemp(st.setpoint, 0)}{tempUnit()}{st.setpoint !== st.target
							? ' (lowered)'
							: ''}
					</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">Heater now · holding</dt>
					<dd class="font-medium text-foreground">
						{st.input.toFixed(0)} % · {st.bias.toFixed(0)} %
					</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">Band</dt>
					<dd class="font-medium text-foreground">±{fmtDelta(st.hysteresis, 2)}{tempUnit()}</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">Sensor noise</dt>
					<dd class="font-medium text-foreground">
						{st.noise != null ? `±${fmtDelta(st.noise, 2)}${tempUnit()}` : '—'}
					</dd>
				</div>
			</dl>
			{#if $deviceId}
				<AutotuneChart deviceId={$deviceId} fromMs={chartFrom} testTemp={st.setpoint} />
			{/if}
			{#if st.message && !running}
				<p class="text-xs {st.phase === 'failed' ? 'text-red-400' : 'text-muted-foreground'}">
					{st.message}
				</p>
			{/if}
		</div>
	{/if}

	{#if running}
		<button
			onclick={stop}
			disabled={loading}
			class="flex w-full items-center justify-center gap-2 rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
		>
			<Square class="h-4 w-4" />{loading ? 'Stopping…' : 'Stop the test'}
		</button>
	{:else if !hasActiveSession}
		<button
			onclick={start}
			disabled={loading || !$deviceId || st?.phase === 'analysing'}
			class="flex w-full items-center justify-center gap-2 rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
		>
			<Play class="h-4 w-4" />{loading ? 'Starting…' : 'Start autotune'}
		</button>
	{/if}

	{#if result && !running && $deviceId}
		<div class="space-y-2">
			<h3 class="text-sm font-semibold text-foreground">Result</h3>
			<TuneResult run={result} deviceId={$deviceId} {current} />
		</div>
	{/if}
</div>
