<script lang="ts">
	/**
	 * Profile drive: replaying the session's profile on the roaster. The
	 * replay runs on the server (docs/replay.md); this panel starts and steers
	 * it, shows the state streamed over the telemetry WebSocket and explains
	 * each option where it is chosen. Driving needs the roaster's full control
	 * (docs/roaster-setup.md); below it only a ghost run (the overlay) is offered.
	 */
	import { profileState } from '#lib/stores/profile.svelte.js';
	import { replayState, setReplayState, telemetry } from '#lib/stores/telemetry.js';
	import { control } from '#lib/api/control.js';
	import {
		replay,
		type ReplayMode,
		type LoopPref,
		type ReplayTrigger,
		type AlignOn,
		type ReplayChannel,
		type ReplayEventView
	} from '#lib/api/replay.js';
	import type { RoastSession } from '#lib/api/sessions.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { controlLevel } from '#lib/stores/roast.svelte.js';
	import { ghostOnlyReason } from '#lib/roaster/setup.js';
	import { fmtTemp, fmtDelta, tempUnit } from '#lib/stores/units.svelte.js';
	import { fmtClock } from '#lib/scope/scopeData.js';
	import {
		MODE_LABELS,
		LOOP_LABELS,
		TRIGGER_LABELS,
		ALIGN_LABELS,
		CHANNEL_LABELS,
		profileHasHeater,
		profileHasFan,
		fmtChannelValue,
		sortChannels,
		fmtEta,
		fmtTrigger,
		fmtLogEntry,
		describeReplayEvent,
		holdNote
	} from '#lib/replay/replay.js';
	import { HELP } from '#lib/constants/help.js';
	import Panel from '#lib/components/ui/Panel.svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { Route, ChevronDown } from '@lucide/svelte';

	let { activeSession, deviceId }: { activeSession: RoastSession | null; deviceId: string | null } =
		$props();

	const OPTS_KEY = 'roastwarden.replay.options';
	interface Opts {
		mode: ReplayMode;
		trigger: ReplayTrigger;
		align: AlignOn;
		loop: LoopPref;
		fan: boolean;
		ghost: boolean;
	}
	function loadOpts(): Opts {
		const d: Opts = {
			mode: 'setpoint',
			trigger: 'profile',
			align: 'charge',
			loop: 'auto',
			fan: true,
			ghost: false
		};
		try {
			const raw = localStorage.getItem(OPTS_KEY);
			if (raw) return { ...d, ...JSON.parse(raw) };
		} catch {
			/* ignore */
		}
		return d;
	}
	let opts = $state<Opts>(loadOpts());
	$effect(() => {
		const snapshot = JSON.stringify(opts);
		try {
			localStorage.setItem(OPTS_KEY, snapshot);
		} catch {
			/* ignore */
		}
	});

	let busy = $state(false);
	let showLog = $state(false);
	let showMore = $state(false);

	const temp = (c: number) => `${fmtTemp(c, 0)}${tempUnit()}`;
	const delta = (c: number) => `${fmtDelta(c, 0)}${tempUnit()}`;

	const st = $derived(
		$replayState && activeSession && $replayState.session_id === activeSession.id
			? $replayState
			: null
	);
	const running = $derived(st !== null && st.active);
	/** Why only a ghost run is offered (control below full), or null. */
	const ghostOnly = $derived(ghostOnlyReason(controlLevel()));
	const ghost = $derived(opts.ghost || ghostOnly !== null);
	// Heater power is a manual switch the replay never turns on: say so when it is off.
	const heaterOff = $derived($telemetry != null && $telemetry.heaterEnable !== 1);
	async function heaterOn() {
		if (!deviceId) return;
		await control
			.setHeaterEnable(deviceId, true)
			.catch((e) =>
				notifications.add(
					`Failed to switch the heater: ${e instanceof Error ? e.message : e}`,
					'error'
				)
			);
	}
	const profile = $derived(profileState.activeProfile);
	const hasHeater = $derived(profileHasHeater(profile));
	const hasFan = $derived(profileHasFan(profile));
	const canStart = $derived(
		profile !== null && activeSession !== null && activeSession.status === 'active' && !!deviceId
	);
	const modeOk = $derived(opts.mode === 'setpoint' || hasHeater);

	// Keep the legacy flags in sync for components that still read them.
	$effect(() => {
		profileState.isFollowing = running;
		profileState.lastSentSetpoint = running ? (st?.last_sent_setpoint ?? null) : null;
	});

	// Restore state after a reload or device switch.
	$effect(() => {
		const sid = activeSession?.id;
		if (!sid) {
			setReplayState(null);
			return;
		}
		replay
			.get(sid)
			.then((s) => {
				if ((s.active || s.stopped_by_device || s.stopped_by_restart) && s.device_id)
					setReplayState(s as never);
			})
			.catch(() => {});
	});

	async function act(label: string, fn: () => Promise<unknown>) {
		busy = true;
		try {
			const res = await fn();
			if (res && typeof res === 'object' && 'session_id' in res) setReplayState(res as never);
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e);
			notifications.add(`${label}: ${msg}`, 'error');
		} finally {
			busy = false;
		}
	}

	function start() {
		if (!activeSession) return;
		const sid = activeSession.id;
		act('Replay', () =>
			replay.start(sid, {
				mode: opts.mode,
				trigger: opts.trigger,
				align: opts.align,
				loop: opts.loop,
				fan: hasFan && opts.fan,
				ghost
			})
		);
	}
	const stop = () => activeSession && act('Stop replay', () => replay.stop(activeSession!.id));
	const engage = () =>
		activeSession && act('Engage', () => replay.update(activeSession!.id, { ghost: false }));
	const setAlign = (a: AlignOn) =>
		activeSession && act('Align', () => replay.update(activeSession!.id, { align: a }));
	const setTrigger = (t: ReplayTrigger) =>
		activeSession && act('Trigger', () => replay.update(activeSession!.id, { trigger: t }));
	const togglePause = (ch: ReplayChannel, paused: boolean) =>
		activeSession &&
		act(paused ? 'Resume' : 'Pause', () =>
			paused ? replay.resume(activeSession!.id, ch) : replay.pause(activeSession!.id, ch)
		);
	const resumeAll = () => activeSession && act('Resume', () => replay.resume(activeSession!.id));
	const eventOp = (e: ReplayEventView, op: 'skip' | 'delay' | 'fire') =>
		activeSession &&
		act(`Event ${op}`, () =>
			replay.eventOp(activeSession!.id, e.id, op, op === 'delay' ? 15 : undefined)
		);

	const anyPaused = $derived(st?.channels.some((c) => c.driven && c.paused) ?? false);
	const SHORT_MODE: Record<ReplayMode, string> = {
		setpoint: 'Setpoint',
		feedforward: 'Feedforward',
		feedforward_pid: 'FF + PID'
	};
	const MODES: { id: ReplayMode; help: typeof HELP.modeSetpoint }[] = [
		{ id: 'setpoint', help: HELP.modeSetpoint },
		{ id: 'feedforward', help: HELP.modeFeedforward },
		{ id: 'feedforward_pid', help: HELP.modeFeedforwardPid }
	];
	const blockedReason = $derived(
		!profile
			? 'Pick a profile for the roast first.'
			: !activeSession
				? 'Start the roast first (Next step), then drive it from here.'
				: activeSession.status !== 'active'
					? 'Resume recording to drive the roaster.'
					: !modeOk
						? 'This profile has no heater data: use Setpoint.'
						: null
	);
	const sel =
		'w-full rounded-md border border-border bg-input px-2 py-1.5 text-xs text-foreground disabled:opacity-50';
	const btn =
		'rounded border border-border px-1.5 py-0.5 text-[11px] text-foreground hover:bg-accent disabled:opacity-40';
</script>

<Panel title="Profile drive" key="replay" open={true}>
	{#snippet icon()}<Route class="h-4 w-4" />{/snippet}
	{#snippet summary()}
		{#if running && st}
			<span
				class="mr-1 rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase {st.config.ghost
					? 'bg-sky-500/15 text-sky-400'
					: 'bg-green-500/15 text-green-400'}">{st.config.ghost ? 'Ghost' : 'Live'}</span
			>
			{MODE_LABELS[st.config.mode]} · {fmtClock(st.profile_t)}
		{:else if profile}
			Off · {profile.name}
		{:else}
			No profile
		{/if}
	{/snippet}

	{#if !running && !profile}
		<p class="text-xs text-muted-foreground">
			Pick a profile in Next step; it can then drive the roaster from here (setpoint, heater and fan
			every second), with a dry run first if you like.
		</p>
	{:else if !running}
		{#if ghostOnly}
			<p class="mb-2 text-xs text-muted-foreground">
				{ghostOnly}
				<Hint inline help={HELP.ghost} />
			</p>
		{:else}
			<p class="mb-2 text-xs text-muted-foreground">
				The server sends the profile's setpoint, heater and fan every second. Or leave this off and
				roast by hand against the overlay.
			</p>
		{/if}

		<div
			class="flex overflow-hidden rounded-md border border-border text-xs"
			role="radiogroup"
			aria-label="Replay mode"
		>
			{#each MODES as m (m.id)}
				{@const disabled = !profile || (m.id !== 'setpoint' && !hasHeater)}
				<button
					type="button"
					role="radio"
					aria-checked={opts.mode === m.id}
					{disabled}
					onclick={() => (opts.mode = m.id)}
					title={disabled && profile ? 'This profile has no heater data' : m.help.line}
					class="flex-1 px-2 py-1.5 font-medium transition-colors disabled:opacity-40
					{opts.mode === m.id ? 'bg-action text-action-foreground' : 'text-muted-foreground hover:bg-accent'}"
				>
					{SHORT_MODE[m.id]}
				</button>
			{/each}
		</div>
		{#each MODES.filter((m) => m.id === opts.mode) as m (m.id)}
			<p class="mt-1 text-xs text-muted-foreground">{m.help.line} <Hint inline help={m.help} /></p>
		{/each}

		<div class="mt-2 flex flex-wrap gap-x-4 gap-y-1">
			{#if !ghostOnly}
				<label class="flex items-center gap-1.5 text-xs">
					<input type="checkbox" class="accent-sky-500" bind:checked={opts.ghost} />
					<span class="font-medium text-foreground">Ghost first (dry run)</span>
					<Hint inline help={HELP.ghost} />
				</label>
			{/if}
			<label
				class="flex items-center gap-1.5 text-xs {hasFan ? '' : 'opacity-50'}"
				title={hasFan ? '' : 'This profile has no fan data'}
			>
				<input
					type="checkbox"
					class="accent-violet-500"
					bind:checked={opts.fan}
					disabled={!hasFan}
				/>
				<span class="font-medium text-foreground">Replay fan</span>
				<Hint inline help={HELP.fanChannel} />
			</label>
		</div>

		<button
			type="button"
			class="mt-3 flex items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground"
			aria-expanded={showMore}
			onclick={() => (showMore = !showMore)}
		>
			<ChevronDown class="h-3.5 w-3.5 transition-transform {showMore ? 'rotate-180' : ''}" /> Timing and
			loop
		</button>
		{#if showMore}
			<div class="mt-2 space-y-3 rounded-md bg-muted/40 p-3">
				<div>
					<label for="rp-trigger" class="text-xs font-medium text-foreground">Events fire</label>
					<select id="rp-trigger" class={sel} bind:value={opts.trigger}>
						{#each Object.entries(TRIGGER_LABELS) as [k, v] (k)}<option value={k}>{v}</option
							>{/each}
					</select>
					<Hint class="mt-1" help={HELP.trigger} />
				</div>
				<div>
					<label for="rp-align" class="text-xs font-medium text-foreground">Align on</label>
					<select id="rp-align" class={sel} bind:value={opts.align}>
						{#each Object.entries(ALIGN_LABELS) as [k, v] (k)}<option value={k}>{v}</option>{/each}
					</select>
					<Hint class="mt-1" help={HELP.align} />
				</div>
				{#if opts.mode !== 'feedforward'}
					<div>
						<label for="rp-loop" class="text-xs font-medium text-foreground">PID runs</label>
						<select id="rp-loop" class={sel} bind:value={opts.loop}>
							{#each Object.entries(LOOP_LABELS) as [k, v] (k)}<option value={k}>{v}</option>{/each}
						</select>
						<Hint class="mt-1" help={HELP.loop} />
					</div>
				{/if}
			</div>
		{/if}

		<button
			onclick={start}
			disabled={!canStart || !modeOk || busy}
			class="mt-3 w-full rounded-md px-3 py-2 text-sm font-semibold disabled:opacity-50 {ghost
				? 'bg-sky-600 text-white hover:bg-sky-700'
				: 'bg-go text-go-foreground hover:bg-go-hover'}"
		>
			{busy ? '…' : ghost ? 'Start ghost run' : 'Drive the roaster'}
		</button>
		{#if blockedReason}
			<p class="mt-1.5 text-xs text-muted-foreground">{blockedReason}</p>
		{/if}
		{#if heaterOff && profile && !ghostOnly}
			<p class="mt-1.5 text-xs text-amber-400">
				Heater power is off; the replay does not switch it on. <button
					class="underline"
					onclick={heaterOn}>Turn it on</button
				>
			</p>
		{/if}
		{#if st && !st.active && st.stopped_reason}
			<p class="mt-1.5 text-xs text-muted-foreground">Last run stopped: {st.stopped_reason}</p>
		{/if}
	{:else if st}
		{#if heaterOff && !st.config.ghost && !ghostOnly}
			<div class="mb-2 flex items-center gap-2 border-l-2 border-red-500 pl-2 text-xs text-red-400">
				<span class="flex-1"
					>Heater power is off, so the profile cannot heat. The replay never switches it on.</span
				>
				<button
					onclick={heaterOn}
					class="shrink-0 rounded-md bg-red-600 px-2 py-1 font-medium text-white hover:bg-red-700"
					>Turn heater on</button
				>
			</div>
		{/if}
		<div class="flex items-center gap-2 text-xs text-muted-foreground">
			<p
				class="min-w-0 flex-1"
				title={st.loop_reason
					? `PID: ${st.loop_reason}${st.lease ? ' · lease held' : ''}`
					: undefined}
			>
				<span class="font-medium {st.config.ghost ? 'text-sky-400' : 'text-green-400'}"
					>{st.config.ghost ? 'Ghost' : 'Live'}</span
				>
				· profile at
				<span class="font-mono text-foreground">{fmtClock(st.profile_t)}</span
				>{#if st.config.hold}<span class="font-mono" title={holdNote(st)}>
						(+{fmtClock(st.config.hold.seconds)})</span
					>{/if}
				{#if st.config.mode !== 'feedforward' && st.lookahead_now != null}
					<span
						title={st.config.adapt_lookahead
							? `Setpoint lead in use; adapts between 0 and ${st.config.lookahead_seconds} s`
							: 'Setpoint lead'}
						>· lead <span class="font-mono text-foreground"
							>{Math.round(st.lookahead_now)}&nbsp;s</span
						></span
					>
				{/if}
				<Hint inline help={st.config.ghost ? HELP.ghost : HELP.live} />
			</p>
			<button
				onclick={stop}
				disabled={busy}
				class="shrink-0 rounded-md bg-action px-2.5 py-1 text-xs font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>Stop</button
			>
		</div>

		{#if st.config.ghost && ghostOnly}
			<p class="mt-2 text-xs text-muted-foreground">{ghostOnly}</p>
		{:else if st.config.ghost}
			<button
				onclick={engage}
				disabled={busy}
				class="mt-2 w-full rounded-md bg-go px-3 py-2 text-sm font-semibold text-go-foreground hover:bg-go-hover disabled:opacity-50"
			>
				Engage: let the profile drive the roaster
			</button>
		{/if}

		<div class="mt-3 space-y-1 text-xs text-muted-foreground [@media(max-height:820px)]:mt-1">
			{#if !st.charge_marked}
				<div class="text-amber-400">Timed from the session start until CHARGE is marked.</div>
			{/if}
			{#if st.loop_reason}
				<div
					class="truncate [@media(max-height:820px)]:hidden"
					title="{st.caps?.reported
						? 'Capabilities reported by the roaster'
						: 'Capabilities inferred from telemetry'}{st.lease
						? '. Control lease held: the roaster goes safe if the server stops.'
						: ''}"
				>
					PID: {st.loop_reason}{#if st.lease}
						· lease held{/if}
				</div>
			{/if}
			{#if st.telemetry_stale}
				<div class="text-amber-400">No fresh telemetry: BT events wait, the PID holds.</div>
			{/if}
			{#if st.last_error}
				<div class="text-red-400">Last send failed: {st.last_error}</div>
			{/if}
		</div>

		<div class="mt-3 grid grid-cols-2 gap-2">
			<div>
				<label for="rp-align-live" class="text-[11px] text-muted-foreground">Align on</label>
				<select
					id="rp-align-live"
					class={sel}
					value={st.config.align}
					onchange={(e) => setAlign(e.currentTarget.value as AlignOn)}
					disabled={busy}
				>
					{#each Object.entries(ALIGN_LABELS) as [k, v] (k)}<option value={k}>{v}</option>{/each}
				</select>
				{#if st.config.align !== 'charge'}
					<div class="mt-0.5 text-[11px] text-muted-foreground">
						{#if st.align.applied}
							shifted {st.align.offset_seconds >= 0 ? '+' : '-'}{fmtClock(
								Math.abs(st.align.offset_seconds)
							)}{#if st.align.offset_c != null}, {fmtDelta(
									st.align.offset_c,
									1,
									true
								)}{tempUnit()}{/if}
						{:else}
							waiting for {ALIGN_LABELS[st.config.align]}
						{/if}
					</div>
				{/if}
			</div>
			<div>
				<label for="rp-trigger-live" class="text-[11px] text-muted-foreground">Events fire</label>
				<select
					id="rp-trigger-live"
					class={sel}
					value={st.config.trigger}
					onchange={(e) => setTrigger(e.currentTarget.value as ReplayTrigger)}
					disabled={busy}
				>
					{#each Object.entries(TRIGGER_LABELS) as [k, v] (k)}<option value={k}>{v}</option>{/each}
				</select>
			</div>
		</div>

		<!-- Channels -->
		<div class="mt-3 border-t border-border pt-2">
			<div class="mb-1 flex items-center gap-1 text-xs font-medium text-muted-foreground">
				Channels <Hint inline help={HELP.channelPause} />
			</div>
			<div class="space-y-1">
				{#each sortChannels(st.channels).filter((c) => c.driven) as c (c.channel)}
					<div class="flex items-center gap-2 text-xs">
						<span class="w-16 text-muted-foreground">{CHANNEL_LABELS[c.channel]}</span>
						<span class="font-mono font-semibold text-foreground"
							>{fmtChannelValue(c.channel, c.target, temp)}</span
						>
						<span class="min-w-0 truncate text-[10px] text-muted-foreground">
							{#if c.paused}<span class="text-amber-400">paused: yours</span>
							{:else if c.channel === 'heater' && st.loop === 'server'}server PID{st.config.mode ===
								'feedforward_pid'
									? ` + ${c.source ?? 'ff'}`
									: ''}
							{:else if c.channel === 'heater' && st.config.mode === 'feedforward_pid'}feedforward ({c.source})
							{:else if c.channel === 'setpoint' && st.loop === 'server'}target ({c.source})
							{:else}{c.source}{/if}
							{#if c.channel === 'fan' && st.fan_assist && st.fan_assist.trim >= 1}
								<span
									class="text-sky-400"
									title="Fan assist: heater pinned, fan trimmed below {Math.round(
										st.fan_assist.ceiling
									)} %"
								>
									assist −{Math.round(st.fan_assist.trim)} %
								</span>
							{/if}
						</span>
						<button
							class="{btn} ml-auto {c.paused ? 'border-amber-500 text-amber-400' : ''}"
							onclick={() => togglePause(c.channel, c.paused)}
							disabled={busy}
							title={c.paused
								? 'Give this channel back to the profile'
								: 'Take this channel back by hand'}
						>
							{c.paused ? 'Resume' : 'Pause'}
						</button>
					</div>
				{/each}
				{#if anyPaused}
					<button class="{btn} w-full" onclick={resumeAll} disabled={busy}>Resume all</button>
				{/if}
			</div>
		</div>

		<!-- Next events -->
		{#if st.upcoming.length > 0}
			<div class="mt-3 border-t border-border pt-2">
				<div class="mb-1 text-xs font-medium text-muted-foreground">Next</div>
				<div class="space-y-1">
					{#each st.upcoming as e, i (e.id)}
						<!-- On a short window only the next two show, so the tab never scrolls mid-drive. -->
						<div
							class="text-xs {i >= 2 ? '[@media(max-height:820px)]:hidden' : ''}"
							title={fmtTrigger(e, temp, delta)}
						>
							<div class="flex items-center gap-2">
								<span class="w-11 font-mono text-foreground">{fmtEta(e)}</span>
								<span class="min-w-0 flex-1 truncate text-foreground"
									>{describeReplayEvent(e, temp)}</span
								>
								{#if e.action !== 'landmark' && e.action !== 'note'}
									<button
										class={btn}
										onclick={() => eventOp(e, 'delay')}
										disabled={busy}
										title="Delay 15 s">+15s</button
									>
									<button
										class={btn}
										onclick={() => eventOp(e, 'fire')}
										disabled={busy}
										title="Fire now">Now</button
									>
								{/if}
								<button class={btn} onclick={() => eventOp(e, 'skip')} disabled={busy} title="Skip"
									>Skip</button
								>
							</div>
							<!-- Below 760 px tall the trigger line is the row's tooltip. -->
							<div
								class="pl-[3.25rem] text-[11px] text-muted-foreground [@media(max-height:760px)]:hidden"
							>
								{fmtTrigger(e, temp, delta)}{e.state === 'waiting' ? ' · delayed' : ''}
							</div>
						</div>
					{/each}
					{#if st.upcoming.length > 2}
						<div class="hidden text-[11px] text-muted-foreground [@media(max-height:820px)]:block">
							+{st.upcoming.length - 2} more on the scope
						</div>
					{/if}
				</div>
			</div>
		{/if}

		<!-- Log tail (left out on short windows; the full log stays with the roast) -->
		{#if st.recent.length > 0}
			<div class="mt-3 border-t border-border pt-2 [@media(max-height:820px)]:hidden">
				<button
					class="text-xs font-medium text-muted-foreground hover:text-foreground"
					onclick={() => (showLog = !showLog)}
				>
					Log {showLog ? '▾' : '▸'}
				</button>
				{#if showLog}
					<div class="mt-1 space-y-0.5 font-mono text-[11px] text-muted-foreground">
						{#each st.recent as l, i (i)}
							<div class="truncate" title={fmtLogEntry(l, temp)}>{fmtLogEntry(l, temp)}</div>
						{/each}
					</div>
				{:else}
					<div class="mt-1 truncate font-mono text-[11px] text-muted-foreground">
						{fmtLogEntry(st.recent[0], temp)}
					</div>
				{/if}
			</div>
		{/if}
	{/if}
</Panel>
