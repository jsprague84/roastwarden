<script lang="ts">
	/**
	 * The scope's header: the readout (BT, ET, RoR against the profile's, Δ profile and the
	 * tracking RMS; the hovered sample's while the cursor is on the chart), the drive's
	 * alignment, the development line, the model's or the projection's forecast, and the
	 * controls: the time axis views, back to live, full screen.
	 */
	import { Maximize2, Minimize2, RotateCcw } from '@lucide/svelte';
	import type { ReplayState } from '#lib/api/replay.js';
	import { deltaLevel } from '#lib/roast/readout.js';
	import { SERIES_COLORS } from '#lib/scope/draw.js';
	import type { ScopeReadout, ScopeStats } from '#lib/scope/model.js';
	import { landmarkText, rorWarningText } from '#lib/scope/prediction.js';
	import { fmtClock, type Num, type ScopeView } from '#lib/scope/scopeData.js';
	import { historyGap } from '#lib/stores/telemetry.js';
	import { fmtDelta, fmtTemp, tempUnit, unitState } from '#lib/stores/units.svelte.js';

	let {
		readout,
		cursor,
		liveReadout,
		replay,
		stats,
		modelShown,
		view,
		hasFrame,
		zoomed,
		fullscreen,
		onview,
		onreset,
		onfullscreen
	}: {
		/** The numbers shown; null before the first sample. */
		readout: ScopeReadout | null;
		/** The cursor is on the chart: the readout is the hovered sample's. */
		cursor: boolean;
		/** Show BT/ET/RoR/Δ when not hovering (off when big readouts sit above the scope). */
		liveReadout: boolean;
		/** The drive running for this roast, for its alignment. */
		replay: ReplayState | null;
		/** The model's numbers now: development, the forecast. */
		stats: ScopeStats | null;
		/** The model's series is on: its forecast is shown instead of the projection's. */
		modelShown: boolean;
		view: ScopeView;
		/** A roast or a preview frames the time axis: the Roast view is available. */
		hasFrame: boolean;
		zoomed: boolean;
		fullscreen: boolean;
		onview: (v: ScopeView) => void;
		onreset: () => void;
		onfullscreen: () => void;
	} = $props();

	const DELTA_CLASS = {
		ok: 'text-green-400',
		warn: 'text-amber-400',
		bad: 'text-red-400'
	} as const;

	const ALIGN_SHORT: Record<string, string> = {
		charge: 'CHARGE',
		turning_point: 'TP',
		drying_end: 'DRY',
		first_crack_start: 'FC'
	};

	// Time axis view: the roast frame during a session, a rolling window otherwise.
	const VIEWS: { id: ScopeView; label: string; title: string }[] = [
		{
			id: 'roast',
			label: 'Roast',
			title:
				'The whole roast: from just before the session started to the profile end (at least 15 min)'
		},
		{ id: 'live10', label: '10 min', title: 'The last 10 minutes' },
		{ id: 'live30', label: '30 min', title: 'The last 30 minutes' },
		{ id: 'all', label: 'All', title: 'Everything this browser holds (up to 2 h)' }
	];

	const fmt1 = (v: Num) => (v == null ? '--' : v.toFixed(1));
</script>

<div class="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-border px-3 py-2 text-xs">
	{#if readout}
		<span class="font-mono text-muted-foreground">{readout.title}</span>
		{#if cursor || liveReadout}
			<span
				><span class="text-muted-foreground">BT</span>
				<span class="font-semibold text-amber-400">{fmt1(readout.bt)}</span></span
			>
			<span
				><span class="text-muted-foreground">ET</span>
				<span class="font-semibold text-blue-400">{fmt1(readout.et)}</span></span
			>
			<span
				><span class="text-muted-foreground">RoR</span>
				<span class="font-semibold text-emerald-400">{fmt1(readout.ror)}</span
				>{#if readout.rorTarget != null}<span
						class="text-muted-foreground"
						title="Profile RoR at this time"
					>
						/ {fmt1(readout.rorTarget)}</span
					>{/if}</span
			>
			{#if readout.delta != null}
				<span
					><span class="text-muted-foreground">Δ profile</span>
					<span
						class="font-semibold {DELTA_CLASS[
							deltaLevel(unitState.unit === 'F' ? readout.delta / 1.8 : readout.delta) ?? 'ok'
						]}">{readout.delta >= 0 ? '+' : ''}{readout.delta.toFixed(1)}</span
					>{#if readout.trackRms != null}<span
							class="text-muted-foreground"
							title="RMS of BT − profile since the turning point"
						>
							(±{readout.trackRms.toFixed(1)} RMS)</span
						>{/if}</span
				>
			{/if}
		{/if}
		{#if !cursor && !liveReadout && readout.trackRms != null}
			<span class="text-muted-foreground" title="RMS of BT − profile since the turning point"
				>tracking ±{readout.trackRms.toFixed(1)} RMS</span
			>
		{/if}
		{#if replay}
			<span
				class="rounded bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground"
				title="Profile alignment used by the running replay"
			>
				{replay.config.ghost ? 'Ghost replay' : 'Replay'}
				{#if replay.align.on !== 'charge'}
					· {#if replay.align.applied}aligned on {ALIGN_SHORT[replay.align.on]}
						{replay.align.offset_seconds >= 0 ? '+' : '-'}{fmtClock(
							Math.abs(replay.align.offset_seconds)
						)}{#if replay.align.offset_c != null}, {fmtDelta(
								replay.align.offset_c,
								1,
								true
							)}{tempUnit()}{/if}{:else}waiting for {ALIGN_SHORT[replay.align.on]}{/if}
				{/if}
			</span>
		{/if}
		{#if !cursor && stats?.dev}
			{@const d = stats.dev}
			<span
				class="rounded bg-red-500/10 px-1.5 py-0.5 text-[11px]"
				title="Development since first crack. DROP time for the target development ratio (the profile's, else Settings); BT then if the RoR held."
			>
				<span class="text-muted-foreground">Dev</span>
				<span
					class="font-semibold {stats.devWindow && d.devSec > stats.devWindow.hi
						? 'text-amber-400'
						: 'text-foreground'}">{fmtClock(d.devSec)}</span
				>
				<span class="font-semibold text-red-400">{d.dtr.toFixed(1)} %</span>
				{#if d.dT != null}<span class="text-muted-foreground">ΔT {fmtDelta(d.dT, 1, true)}</span
					>{/if}
				{#if stats.devWindow}{@const w = stats.devWindow}<span
						class="text-muted-foreground"
						title="Drop {w.lo}–{w.hi} s after first crack: the profile’s expected development (roast guidance)"
						>· aim {fmtClock(w.lo)}–{fmtClock(w.hi)}</span
					>{/if}
				<span class="text-muted-foreground">· DROP {d.targetDtr.toFixed(0)} %</span>
				<span class="font-semibold {d.secsToDrop < 0 ? 'text-red-400' : 'text-foreground'}"
					>{d.secsToDrop < 0
						? `${fmtClock(-d.secsToDrop)} ago`
						: `in ${fmtClock(d.secsToDrop)}`}</span
				>
				{#if d.btAtDrop != null && d.secsToDrop > 0}<span class="text-muted-foreground"
						>at ~{fmtTemp(d.btAtDrop, 0)}{tempUnit()}</span
					>{/if}
				{#if d.targetDropBt != null && d.secsToDropBt != null}<span
						class="text-muted-foreground"
						title="When BT reaches the profile's drop temperature at the RoR now"
						>· {fmtTemp(d.targetDropBt, 0)}{tempUnit()}</span
					>
					<span class="font-semibold text-foreground">in ~{fmtClock(d.secsToDropBt)}</span>{/if}
			</span>
		{/if}
		{#if !cursor && modelShown && stats?.prediction}
			{@const p = stats.prediction}
			{@const fc = landmarkText(p.first_crack)}
			{@const drop = landmarkText(p.drop)}
			<span
				class="text-muted-foreground"
				data-testid="scope-model"
				title={[p.reason, p.first_crack?.reason, p.drop?.reason].filter(Boolean).join('\n')}
			>
				<span style="color:{SERIES_COLORS.model}">model</span> → {fmtTemp(p.bt_60, 0)} in 1:00 · {fmtTemp(
					p.bt_120,
					0
				)} ±{fmtDelta(p.band_120, 0)} in 2:00{#if fc}{' · '}FC {fc}{/if}{#if drop}{' · '}DROP {drop}{/if}{#if p.heater_limit}{' · '}<span
						class="text-highlight">heater 100 % at {fmtClock(p.heater_limit.t)}</span
					>{/if}{#each p.ror?.warnings ?? [] as w (w.kind)}{' · '}<span
						class="text-highlight"
						data-testid="scope-ror-warning"
						title={w.reason}>{rorWarningText(w)}</span
					>{/each}
			</span>
		{:else if !cursor && stats?.proj60 != null}
			<span class="text-muted-foreground">
				→ {fmtTemp(stats.proj60, 0)} in 1:00 · {fmtTemp(stats.proj120 ?? null, 0)} in 2:00
				{#if stats.etaFc != null}
					· {fmtTemp(stats.fcTarget, 0)}{tempUnit()} in ~{fmtClock(stats.etaFc)}
				{/if}
			</span>
		{/if}
		{#if $historyGap}
			<span
				class="rounded bg-amber-500/15 px-1.5 py-0.5 text-amber-400"
				title="The server could not fill a reconnect gap; older points may be missing.">gap</span
			>
		{/if}
	{:else}
		<span class="text-muted-foreground">Waiting for telemetry…</span>
	{/if}
	<span class="ml-auto flex items-center gap-1">
		<span
			class="mr-1 flex overflow-hidden rounded border border-border"
			role="group"
			aria-label="Time axis"
		>
			{#each VIEWS as v (v.id)}
				<button
					type="button"
					title={v.title}
					disabled={v.id === 'roast' && !hasFrame}
					onclick={() => onview(v.id)}
					class="px-2 py-0.5 text-[11px] transition-colors disabled:opacity-40 {view === v.id &&
					!zoomed
						? 'bg-selected text-selected-foreground'
						: 'text-muted-foreground hover:bg-accent hover:text-foreground'}"
					aria-pressed={view === v.id && !zoomed}>{v.label}</button
				>
			{/each}
		</span>
		{#if zoomed}
			<button
				type="button"
				class="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
				title="Back to live (double-click the chart)"
				onclick={onreset}
			>
				<RotateCcw class="h-3.5 w-3.5" />
			</button>
		{/if}
		<button
			type="button"
			class="rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
			title={fullscreen ? 'Exit full screen' : 'Full screen'}
			onclick={onfullscreen}
		>
			{#if fullscreen}<Minimize2 class="h-3.5 w-3.5" />{:else}<Maximize2 class="h-3.5 w-3.5" />{/if}
		</button>
	</span>
</div>
