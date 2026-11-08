<script lang="ts">
	/**
	 * Hands-on control of the selected roaster: mode, setpoint, fan (with fan
	 * assist), heater and the heater switch; PID gains under Advanced. Each
	 * control says what it does in the current mode. The roaster's control level
	 * (docs/roaster-setup.md) decides which show: off, none; manual, fan and heater; and its
	 * outputs too: an undeclared one is hidden, a declared one has its label, range and step.
	 * Drum speed and the damper show only when declared (no kind declares them), one row each.
	 */
	import { control } from '#lib/api/control.js';
	import { resolve } from '$app/paths';
	import { telemetry, replayState } from '#lib/stores/telemetry.js';
	import { notifyError } from '#lib/stores/notifications.js';
	import { roast, selectedDevice, controlLevel, roasterOutputs } from '#lib/stores/roast.svelte.js';
	import { allows, CONTROL_OFF_MESSAGE, sliderStep } from '#lib/roaster/setup.js';
	import type { RoasterOutput } from '#lib/api/roasters.js';
	import {
		unitState,
		toDisplay,
		fromDisplay,
		roundForInput,
		tempUnit
	} from '#lib/stores/units.svelte.js';
	import { fanAssistState, saveFanAssist } from '#lib/stores/fanAssist.svelte.js';
	import { assistSummary, isTrimming } from '#lib/fanAssist/fanAssist.js';
	import { HELP } from '#lib/constants/help.js';
	import Panel from '#lib/components/ui/Panel.svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { SlidersHorizontal, ChevronDown, ArrowRight } from '@lucide/svelte';
	import { isOn } from '#lib/stores/modules.svelte.js';

	const dev = $derived(roast.deviceId);
	const device = $derived(selectedDevice());
	const t = $derived($telemetry);
	const level = $derived(controlLevel());
	const outs = $derived(roasterOutputs());
	const heaterOut = $derived(outs.outputs.heater);
	const fanOut = $derived(outs.outputs.fan);
	/** Drum speed and damper: null unless the setup declares them (an older server leaves them out). */
	const drumOut = $derived(outs.outputs.drum ?? null);
	const damperOut = $derived(outs.outputs.damper ?? null);
	const floor = $derived(outs.fan_floor_pct);
	/** Setpoint, mode and PID gains: full control and a heater (they drive it). */
	const full = $derived(allows(level, 'setpoint') && !!heaterOut);
	/** Fan assist: full control and both outputs. */
	const assistOk = $derived(full && !!fanOut);

	// Setpoint input is in the display unit; converted to °C when sent.
	let setpointValue = $state(200);
	let fanValue = $state(50);
	let heaterValue = $state(0);
	let drumValue = $state(0);
	let damperValue = $state(0);
	let heaterEnabled = $state(false);
	let mode = $state<'manual' | 'auto'>('manual');
	/** One timer per control, so a quick change of one does not drop another's. */
	// eslint-disable-next-line svelte/prefer-svelte-reactivity -- timer handles, never rendered
	const debounceTimers = new Map<string, ReturnType<typeof setTimeout>>();
	let syncedFor: string | null = null;
	/** Ignore telemetry for a moment after a command, so the old value does not snap back. */
	let holdUntil = 0;

	const isAuto = $derived(mode === 'auto');
	const replayLive = $derived(
		!!$replayState?.active &&
			!$replayState.config.ghost &&
			$replayState.session_id === roast.session?.id
	);

	$effect(() => {
		const tm = t;
		if (!tm || !dev) return;
		const first = syncedFor !== dev;
		if (first || Date.now() > holdUntil) {
			mode = tm.controlMode === 1 ? 'auto' : 'manual';
			heaterEnabled = tm.heaterEnable === 1;
			// Follow the roaster (Profile drive, the roast sequence and alarms change these too),
			// except while you are typing a setpoint or dragging the fan.
			const editing = typeof document !== 'undefined' ? document.activeElement?.id : undefined;
			if ((first || editing !== 'setpoint-input') && tm.setpoint != null)
				setpointValue = roundForInput(toDisplay(tm.setpoint));
			else if (first) setpointValue = roundForInput(toDisplay(200));
			if ((first || editing !== 'fan-slider') && tm.fan != null) fanValue = Math.round(tm.fan);
			else if (first) fanValue = 50;
			// The heater too: the PID moves it in Auto, and Drop or Cancel set Manual at 0 %
			if ((first || editing !== 'heater-slider') && tm.heater != null)
				heaterValue = Math.round(tm.heater);
			else if (first) heaterValue = 0;
			if ((first || editing !== 'drum-slider') && tm.drum != null) drumValue = Math.round(tm.drum);
			else if (first) drumValue = 0;
			if ((first || editing !== 'damper-slider') && tm.damper != null)
				damperValue = Math.round(tm.damper);
			else if (first) damperValue = 0;
			syncedFor = dev;
		}
	});

	const assist = $derived(fanAssistState.deviceId === dev ? fanAssistState.view : null);
	const assistLine = $derived(assistSummary(assist));
	let assistBusy = $state(false);

	async function toggleAssist() {
		if (!dev || !assist) return;
		assistBusy = true;
		await saveFanAssist(dev, { ...assist.config, enabled: !assist.config.enabled })
			.catch(notifyError('Failed to switch fan assist'))
			.finally(() => (assistBusy = false));
	}

	function hold() {
		holdUntil = Date.now() + 3000;
	}

	function debounce(key: string, fn: () => void, ms = 300) {
		const old = debounceTimers.get(key);
		if (old) clearTimeout(old);
		debounceTimers.set(key, setTimeout(fn, ms));
	}

	// Re-express the current setpoint when the unit preference changes.
	let lastUnit = unitState.unit;
	$effect(() => {
		const u = unitState.unit;
		if (u === lastUnit) return;
		const c = lastUnit === 'F' ? (setpointValue - 32) / 1.8 : setpointValue;
		lastUnit = u;
		setpointValue = roundForInput(u === 'F' ? c * 1.8 + 32 : c);
	});

	const spMax = $derived(roundForInput(toDisplay(250)));
	const spMin = $derived(roundForInput(toDisplay(0)));
	const spStep = $derived(unitState.unit === 'F' ? 10 : 5);

	async function setMode(m: 'manual' | 'auto') {
		if (!dev || m === mode) return;
		mode = m;
		hold();
		await control.setMode(dev, m).catch(notifyError('Failed to set mode'));
	}

	function adjustSetpoint(delta: number) {
		setpointValue = Math.max(spMin, Math.min(spMax, setpointValue + delta));
		sendSetpoint();
	}

	async function sendSetpoint() {
		if (!dev) return;
		hold();
		const celsius = Math.round(fromDisplay(setpointValue) * 10) / 10;
		await control.setSetpoint(dev, celsius).catch(notifyError('Failed to set setpoint'));
	}

	function onFanChange() {
		if (!dev) return;
		hold();
		const d = dev;
		debounce('fan', () => control.setFan(d, fanValue).catch(notifyError('Failed to set fan')));
	}

	function onHeaterChange() {
		if (!dev) return;
		hold();
		const d = dev;
		debounce('heater', () =>
			control.setHeater(d, heaterValue).catch(notifyError('Failed to set heater'))
		);
	}

	function onDrumChange() {
		if (!dev) return;
		hold();
		const d = dev;
		debounce('drum', () =>
			control.setDrum(d, drumValue).catch(notifyError('Failed to set drum speed'))
		);
	}

	function onDamperChange() {
		if (!dev) return;
		hold();
		const d = dev;
		debounce('damper', () =>
			control.setDamper(d, damperValue).catch(notifyError('Failed to set damper'))
		);
	}

	const DRUM_HELP = {
		line: 'Drum speed, %. Profile drive, alarms and the roast sequence do not set it yet.'
	};
	const DAMPER_HELP = {
		line: 'Damper (airflow), %. Profile drive, alarms and the roast sequence do not set it yet.'
	};

	async function toggleHeater() {
		if (!dev) return;
		heaterEnabled = !heaterEnabled;
		hold();
		await control
			.setHeaterEnable(dev, heaterEnabled)
			.catch(notifyError('Failed to switch the heater'));
	}

	// ---- PID gains (Advanced) ----
	let showAdvanced = $state(false);
	let kp = $state(15);
	let ki = $state(1);
	let kd = $state(25);
	let editingGains = $state(false);
	let gainsHoldUntil = 0;
	$effect(() => {
		const tm = t;
		if (!tm || editingGains || Date.now() < gainsHoldUntil) return;
		if (tm.Kp != null) kp = Math.round(tm.Kp * 100) / 100;
		if (tm.Ki != null) ki = Math.round(tm.Ki * 10000) / 10000;
		if (tm.Kd != null) kd = Math.round(tm.Kd * 100) / 100;
	});
	async function applyPid() {
		if (!dev) return;
		gainsHoldUntil = Date.now() + 3000;
		await control.setPid(dev, kp, ki, kd).catch(notifyError('Failed to apply PID'));
	}
	function gainsFocusOut(e: FocusEvent) {
		const box = e.currentTarget as HTMLElement;
		if (!box.contains(e.relatedTarget as Node)) editingGains = false;
	}

	const seg = 'flex-1 px-3 py-1.5 text-sm font-medium transition-colors disabled:opacity-40';
	const gainField =
		'w-full rounded border border-border bg-input px-2 py-1 text-xs text-foreground';
	/** Less space between the controls on a short window (the Roast page fits it without scrolling). */
	const tight = '[@media(max-height:760px)]:mt-2';
	const small =
		'rounded-md border border-border px-2 py-1 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-40';
</script>

{#snippet outputRow(
	id: 'drum-slider' | 'damper-slider',
	o: RoasterOutput,
	help: { line: string },
	text: string,
	accent: string
)}
	<div class="mt-3 flex items-center gap-2 {tight}">
		<span class="flex w-32 shrink-0 items-center gap-1 text-xs text-muted-foreground">
			<label for={id} class="truncate font-medium" title={o.label}>{o.label}</label>
			<span class="font-semibold tabular-nums {text}"
				>{id === 'drum-slider' ? drumValue : damperValue} %</span
			>
			<Hint inline {help} />
		</span>
		{#if id === 'drum-slider'}
			<input
				{id}
				type="range"
				bind:value={drumValue}
				oninput={onDrumChange}
				min="0"
				max={o.max}
				step={sliderStep(o)}
				class="min-w-0 flex-1 {accent}"
			/>
		{:else}
			<input
				{id}
				type="range"
				bind:value={damperValue}
				oninput={onDamperChange}
				min="0"
				max={o.max}
				step={sliderStep(o)}
				class="min-w-0 flex-1 {accent}"
			/>
		{/if}
	</div>
{/snippet}

<Panel title="Roaster control" key="controls" open={true}>
	{#snippet icon()}<SlidersHorizontal class="h-4 w-4" />{/snippet}
	{#snippet summary()}
		{#if level === 'off'}Control off ·
		{/if}{isAuto ? 'Auto (PID)' : 'Manual'} · fan {t?.fan != null ? Math.round(t.fan) : '—'} %{#if isTrimming(assist)}
			(assist){/if} · heater {heaterEnabled
			? `${t?.heater != null ? Math.round(t.heater) : '—'} %`
			: 'off'}
	{/snippet}

	{#if !dev}
		<p class="text-sm text-muted-foreground">Pick a roaster first.</p>
	{:else if !allows(level, 'sliders')}
		<p class="text-sm text-muted-foreground">{CONTROL_OFF_MESSAGE}</p>
		{#if device}
			<a
				href={resolve('/roasters/[id]/control', { id: device.id })}
				class="mt-2 inline-flex items-center gap-1 text-xs font-medium text-foreground underline-offset-2 hover:underline"
			>
				Its setup <ArrowRight class="h-3 w-3" />
			</a>
		{/if}
	{:else if !heaterOut && !fanOut && !drumOut && !damperOut}
		<p class="text-sm text-muted-foreground">
			This roaster has no outputs: Roastwarden only records it. STOP still works.
		</p>
		{#if device}
			<a
				href={resolve('/roasters/[id]/control', { id: device.id })}
				class="mt-2 inline-flex items-center gap-1 text-xs font-medium text-foreground underline-offset-2 hover:underline"
			>
				Its setup <ArrowRight class="h-3 w-3" />
			</a>
		{/if}
	{:else}
		{#if replayLive}
			<p
				class="mb-2 border-l-2 border-sky-500 pl-2 text-xs text-sky-400 [@media(max-height:760px)]:mb-1"
			>
				Profile drive is on: a change here pauses that channel.
				<Hint inline help={HELP.channelPause} />
			</p>
		{/if}

		{#if full}
			<!-- Mode -->
			<div
				class="flex overflow-hidden rounded-md border border-border"
				role="group"
				aria-label="Control mode"
			>
				<button
					class="{seg} {mode === 'manual'
						? 'bg-selected text-selected-foreground'
						: 'text-muted-foreground hover:bg-accent'}"
					aria-pressed={mode === 'manual'}
					onclick={() => setMode('manual')}>Manual</button
				>
				<button
					class="{seg} {mode === 'auto'
						? 'bg-action text-action-foreground'
						: 'text-muted-foreground hover:bg-accent'}"
					aria-pressed={mode === 'auto'}
					onclick={() => setMode('auto')}>Auto (PID)</button
				>
			</div>
			<!-- The mode's one line; a window under 760 px tall needs the room for the controls -->
			<p class="mt-1 text-xs text-muted-foreground [@media(max-height:760px)]:hidden">
				{(isAuto ? HELP.auto : HELP.manual).line}
				<Hint inline help={isAuto ? HELP.auto : HELP.manual} />
			</p>

			<!-- Setpoint -->
			<div
				class="mt-3 flex items-center justify-between gap-2 {tight} {isAuto ? '' : 'opacity-60'}"
			>
				<span class="flex items-center gap-1 text-xs text-muted-foreground">
					<label for="setpoint-input" class="font-medium">Setpoint</label>
					{#if !isAuto}<span>(Auto only)</span>{/if}
					<Hint inline help={HELP.setpoint} />
				</span>
				<div class="flex items-center gap-1.5">
					<button
						onclick={() => adjustSetpoint(-spStep)}
						aria-label="Decrease setpoint by {spStep}{tempUnit()}"
						class={small}>−{spStep}</button
					>
					<div class="relative">
						<input
							id="setpoint-input"
							type="number"
							inputmode="decimal"
							bind:value={setpointValue}
							onchange={sendSetpoint}
							min={spMin}
							max={spMax}
							step="1"
							class="w-[6.75rem] rounded-md border border-border bg-input py-1 pl-2 pr-7 text-center text-base font-semibold tabular-nums text-foreground"
						/>
						<span
							class="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-xs text-muted-foreground"
							>{tempUnit()}</span
						>
					</div>
					<button
						onclick={() => adjustSetpoint(spStep)}
						aria-label="Increase setpoint by {spStep}{tempUnit()}"
						class={small}>+{spStep}</button
					>
				</div>
			</div>
		{:else if heaterOut}
			<p class="text-xs text-muted-foreground">
				Manual control: {[heaterOut?.label, fanOut?.label, drumOut?.label, damperOut?.label]
					.filter(Boolean)
					.join(' and ')
					.toLowerCase()}. Setpoint, mode and PID need full control{#if device}
					(<a
						href={resolve('/roasters/[id]/control', { id: device.id })}
						class="font-medium text-foreground underline-offset-2 hover:underline">setup</a
					>){/if}.
			</p>
		{/if}

		{#if fanOut}
			<!-- Fan -->
			<div class="mt-3 {tight}">
				<div class="flex items-center justify-between gap-2">
					<span class="flex items-center gap-1 text-xs text-muted-foreground">
						<label for="fan-slider" class="font-medium">{fanOut.label}</label>
						<span class="font-semibold tabular-nums text-violet-400">{fanValue} %</span>
						{#if assistOk && assist?.config.enabled}<span class="text-sky-400">ceiling</span>{/if}
						<Hint
							inline
							help={assistOk && assist?.config.enabled
								? HELP.fanAssist
								: {
										line:
											floor > 0
												? `Airflow. While the heater is on it stays at ${floor} % or more.`
												: 'Airflow.',
										more: HELP.fanAssist.more
									}}
						/>
					</span>
					{#if assist && assistOk && isOn('tuning')}
						<span class="flex items-center gap-1.5 text-xs text-muted-foreground">
							<span class={assist.config.enabled ? 'text-sky-400' : ''}>Fan assist</span>
							<button
								type="button"
								role="switch"
								aria-checked={assist.config.enabled}
								aria-label="Fan assist"
								onclick={toggleAssist}
								disabled={assistBusy}
								class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors disabled:opacity-40 {assist
									.config.enabled
									? 'bg-sky-500'
									: 'bg-muted'}"
							>
								<span
									class="inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform {assist
										.config.enabled
										? 'translate-x-[1.1rem]'
										: 'translate-x-1'}"
								></span>
							</button>
						</span>
					{/if}
				</div>
				<input
					id="fan-slider"
					type="range"
					bind:value={fanValue}
					oninput={onFanChange}
					min="0"
					max={fanOut.max}
					step={sliderStep(fanOut)}
					class="mt-1 w-full accent-violet-500"
				/>
				{#if assistLine && assistOk}
					<!-- One line on a short window (the Roast page does not scroll); the whole line on hover -->
					<p
						class="text-[11px] [@media(max-height:820px)]:truncate {isTrimming(assist)
							? 'text-sky-400'
							: 'text-muted-foreground'}"
						title={assistLine}
					>
						{assistLine}
					</p>
				{/if}
			</div>
		{/if}

		{#if heaterOut}
			<!-- Heater -->
			<div class="mt-3 {tight}">
				<div class="flex items-center justify-between gap-2">
					<span class="flex items-center gap-1 text-xs text-muted-foreground">
						<label for="heater-slider" class="font-medium">{heaterOut.label}</label>
						<span class="font-semibold tabular-nums text-red-400">{heaterValue} %</span>
						{#if isAuto}<span class="text-amber-400">set by the PID</span>{/if}
					</span>
					<span class="flex items-center gap-1.5 text-xs text-muted-foreground">
						<span class={heaterEnabled ? 'text-red-400' : ''}>Power</span>
						<Hint inline help={HELP.heaterEnable} />
						<button
							type="button"
							role="switch"
							aria-checked={heaterEnabled}
							aria-label="Heater power"
							onclick={toggleHeater}
							class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {heaterEnabled
								? 'bg-red-500'
								: 'bg-muted'}"
						>
							<span
								class="inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform {heaterEnabled
									? 'translate-x-[1.1rem]'
									: 'translate-x-1'}"
							></span>
						</button>
					</span>
				</div>
				<input
					id="heater-slider"
					type="range"
					bind:value={heaterValue}
					oninput={onHeaterChange}
					min="0"
					max={heaterOut.max}
					step={sliderStep(heaterOut)}
					disabled={isAuto}
					class="mt-1 w-full accent-red-500 disabled:opacity-50"
				/>
			</div>
		{/if}

		{#if drumOut}
			<!-- Drum speed: one row (the Roast page does not scroll) -->
			{@render outputRow('drum-slider', drumOut, DRUM_HELP, 'text-amber-400', 'accent-amber-500')}
		{/if}
		{#if damperOut}
			<!-- Damper: one row -->
			{@render outputRow(
				'damper-slider',
				damperOut,
				DAMPER_HELP,
				'text-teal-400',
				'accent-teal-500'
			)}
		{/if}

		{#if full}
			<!-- Advanced -->
			<button
				type="button"
				class="mt-3 flex items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground {tight}"
				aria-expanded={showAdvanced}
				onclick={() => (showAdvanced = !showAdvanced)}
			>
				<ChevronDown class="h-3.5 w-3.5 transition-transform {showAdvanced ? 'rotate-180' : ''}" /> Advanced:
				PID gains
			</button>
			{#if showAdvanced}
				<div
					class="mt-2 space-y-2 rounded-md bg-muted/40 p-3"
					onfocusin={() => (editingGains = true)}
					onfocusout={gainsFocusOut}
				>
					<div class="grid grid-cols-3 gap-2">
						<div>
							<label for="pid-kp" class="text-[11px] font-medium text-amber-400">Kp</label>
							<input id="pid-kp" type="number" bind:value={kp} step="0.1" class={gainField} />
						</div>
						<div>
							<label for="pid-ki" class="text-[11px] font-medium text-amber-400">Ki</label>
							<input id="pid-ki" type="number" bind:value={ki} step="0.01" class={gainField} />
						</div>
						<div>
							<label for="pid-kd" class="text-[11px] font-medium text-amber-400">Kd</label>
							<input id="pid-kd" type="number" bind:value={kd} step="0.1" class={gainField} />
						</div>
					</div>
					<button
						onclick={applyPid}
						class="w-full rounded-md bg-action px-3 py-1.5 text-xs font-medium text-action-foreground hover:bg-action-hover"
						>Apply gains now</button
					>
					<Hint
						help={{
							line: 'Applied to the roaster at once. With the gain schedule on, the next fan change replaces them.',
							more: HELP.gainSchedule.more
						}}
					/>
					{#if device}
						<a
							href={resolve('/roasters/[id]/control', { id: device.id })}
							class="inline-flex items-center gap-1 text-xs font-medium text-foreground underline-offset-2 hover:underline"
						>
							{isOn('tuning')
								? 'Gain schedule, fan assist settings, autotune'
								: 'The roaster’s control'}
							<ArrowRight class="h-3 w-3" />
						</a>
					{/if}
				</div>
			{/if}
		{/if}
	{/if}
</Panel>
