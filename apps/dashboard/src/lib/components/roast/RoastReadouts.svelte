<script lang="ts">
	/**
	 * The numbers you roast by, in one strip: time, BT, ET, RoR (live /
	 * profile) and Δ to the profile large; setpoint, heater, fan and
	 * development small. Colours match the scope's series.
	 */
	import { telemetry, rateOfRise, replayState } from '#lib/stores/telemetry.js';
	import { profileState } from '#lib/stores/profile.svelte.js';
	import { roast } from '#lib/stores/roast.svelte.js';
	import { fanAssistState } from '#lib/stores/fanAssist.svelte.js';
	import { isTrimming } from '#lib/fanAssist/fanAssist.js';
	import { fmtTemp, fmtDelta, tempUnit, rorUnit } from '#lib/stores/units.svelte.js';
	import { fmtClock } from '#lib/scope/scopeData.js';
	import { profileTrack, roastNumbers, deltaLevel } from '#lib/roast/readout.js';

	let now = $state(Date.now());
	$effect(() => {
		const t = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(t);
	});

	const t = $derived($telemetry);
	const session = $derived(roast.session);
	const replay = $derived(
		$replayState && $replayState.active && session && $replayState.session_id === session.id
			? $replayState
			: null
	);
	const track = $derived(
		profileTrack(profileState.activeProfile, replay?.align?.offset_seconds ?? 0)
	);
	const n = $derived(
		roastNumbers({ nowMs: now, session, events: roast.events, track, bt: t?.beanTemp })
	);
	const level = $derived(deltaLevel(n.delta));
	const levelClass = $derived(
		level === 'ok'
			? 'text-green-400'
			: level === 'warn'
				? 'text-amber-400'
				: level === 'bad'
					? 'text-red-400'
					: 'text-muted-foreground'
	);
	const auto = $derived(t?.controlMode === 1);
	const assist = $derived(fanAssistState.deviceId === roast.deviceId ? fanAssistState.view : null);
	const trim = $derived(isTrimming(assist) ? Math.round(assist?.state.view?.trim ?? 0) : 0);
	const dropped = $derived(roast.events.some((e) => e.event_type === 'drop'));
</script>

{#snippet cell(label: string, value: string, unit: string, color: string, sub?: string, cls = '')}
	<div class="min-w-0 bg-card px-3 py-2 {cls}">
		<div class="text-[11px] text-muted-foreground">{label}</div>
		<div class="mt-0.5 flex items-baseline gap-1 whitespace-nowrap">
			<span class="text-3xl font-semibold tabular-nums leading-none xl:text-4xl {color}"
				>{value}</span
			>
			<span class="text-xs text-muted-foreground">{unit}</span>
		</div>
		<div class="mt-1 h-4 truncate text-xs tabular-nums text-muted-foreground">{sub ?? ''}</div>
	</div>
{/snippet}

<!-- One strip, cells split by hairlines (the grid gap shows the border colour). -->
<div
	class="grid grid-cols-2 gap-px overflow-hidden rounded-md border border-border bg-border sm:grid-cols-3 lg:grid-cols-[auto_repeat(4,minmax(0,1fr))_auto]"
>
	{@render cell(
		n.sinceCharge != null ? 'Since charge' : session ? 'Recording' : 'Idle',
		session ? fmtClock(n.sinceCharge ?? n.sinceStart) : '—',
		'',
		'font-mono text-foreground',
		n.devSec != null
			? `dev ${fmtClock(n.devSec)}`
			: session && n.sinceCharge == null
				? 'not charged'
				: undefined,
		'order-5 sm:order-none'
	)}
	{@render cell(
		'Bean temp',
		fmtTemp(t?.beanTemp),
		tempUnit(),
		'text-amber-400',
		n.profBt != null ? `profile ${fmtTemp(n.profBt)}` : undefined,
		'order-1 sm:order-none'
	)}
	{@render cell(
		'Env temp',
		fmtTemp(t?.envTemp),
		tempUnit(),
		'text-blue-400',
		undefined,
		'order-3 sm:order-none'
	)}
	{@render cell(
		'RoR',
		fmtDelta($rateOfRise),
		rorUnit(),
		'text-emerald-400',
		n.profRor != null ? `profile ${fmtDelta(n.profRor)}` : undefined,
		'order-2 sm:order-none'
	)}
	{@render cell(
		'Δ profile',
		n.delta != null ? fmtDelta(n.delta, 1, true) : '—',
		n.delta != null ? tempUnit() : '',
		levelClass,
		n.delta != null
			? n.delta >= 0
				? 'ahead'
				: 'behind'
			: profileState.activeProfile
				? session
					? 'outside the profile'
					: 'from the roast start'
				: 'no profile',
		'order-4 sm:order-none'
	)}

	<!-- Controls and development, small -->
	<dl
		class="order-6 grid grid-cols-[auto_1fr] content-center gap-x-3 gap-y-0.5 bg-card px-3 py-2 text-xs tabular-nums sm:order-none"
	>
		<dt class="text-muted-foreground">SP</dt>
		<dd class="whitespace-nowrap">
			<span class="font-semibold text-orange-400">{fmtTemp(t?.setpoint, 0)}{tempUnit()}</span>
			<span
				class="ml-1 text-[10px] font-semibold uppercase {auto
					? 'text-amber-400'
					: 'text-muted-foreground'}">{auto ? 'Auto' : 'Manual'}</span
			>
		</dd>
		<dt class="text-muted-foreground">Heater</dt>
		<dd class="whitespace-nowrap">
			<span class="font-semibold text-red-400"
				>{t?.heater != null ? Math.round(t.heater) : '—'} %</span
			>
			{#if t && t.heaterEnable !== 1}<span class="ml-1 text-muted-foreground">off</span>{/if}
		</dd>
		<dt class="text-muted-foreground">Fan</dt>
		<dd class="whitespace-nowrap">
			<span class="font-semibold text-violet-400">{t?.fan != null ? Math.round(t.fan) : '—'} %</span
			>
			{#if trim > 0}<span class="ml-1 text-sky-400" title="Fan assist has taken this much air away"
					>−{trim}</span
				>{/if}
		</dd>
		<dt class="text-muted-foreground" title="Development: share of the roast since first crack">
			DTR
		</dt>
		<dd class="whitespace-nowrap">
			{#if n.dtr != null}
				<span class="font-semibold text-red-400">{n.dtr.toFixed(1)} %</span>
				{#if track?.targetDtr != null && !dropped}<span class="ml-1 text-muted-foreground"
						>/ {track.targetDtr.toFixed(0)}</span
					>{/if}
			{:else}<span class="text-muted-foreground">—</span>{/if}
		</dd>
	</dl>
</div>
