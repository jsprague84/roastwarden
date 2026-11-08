<script lang="ts">
	/** Autotune runs per device and the stored-gain history with undo. */
	import { History, Undo2 } from '@lucide/svelte';
	import { deviceId, telemetry } from '#lib/stores/telemetry.js';
	import { tuning, type AutotuneRun, type PidChange, type Gains } from '#lib/api/tuning.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { fmtTemp, tempUnit } from '#lib/stores/units.svelte.js';
	import {
		qualityInfo,
		toneText,
		fmtGains,
		sourceLabel,
		errorText,
		ruleLabel
	} from '#lib/autotune/tuning.js';
	import { tuningBus, bump } from '#lib/autotune/bus.svelte.js';
	import AutotuneRunCard from '#lib/components/roasters/AutotuneRunCard.svelte';
	import TuneResult from '#lib/components/roasters/TuneResult.svelte';

	let runs = $state<AutotuneRun[]>([]);
	let changes = $state<PidChange[]>([]);
	let openRun = $state<string | null>(null);
	let undoing = $state(false);
	let showAllChanges = $state(false);

	const current = $derived<Gains | null>(
		$telemetry?.Kp != null && $telemetry?.Ki != null && $telemetry?.Kd != null
			? { kp: $telemetry.Kp, ki: $telemetry.Ki, kd: $telemetry.Kd }
			: null
	);
	const canUndo = $derived(changes.some((c) => !c.undone && c.source !== 'undo' && c.previous));

	$effect(() => {
		void tuningBus.runs;
		const id = $deviceId;
		if (!id) return;
		const t = setTimeout(() => loadRuns(id), 700);
		return () => clearTimeout(t);
	});

	$effect(() => {
		void tuningBus.pid;
		const id = $deviceId;
		if (!id) return;
		// Telemetry reports the new gains about a second after a change.
		const t = setTimeout(() => loadChanges(id), 300);
		return () => clearTimeout(t);
	});

	async function loadRuns(id: string) {
		try {
			runs = (await tuning.runs(id, 30)).runs;
		} catch {
			runs = [];
		}
	}

	async function loadChanges(id: string) {
		try {
			changes = (await tuning.pidHistory(id, 30)).changes;
		} catch {
			changes = [];
		}
	}

	async function undo() {
		if (!$deviceId) return;
		undoing = true;
		try {
			const r = await tuning.undo($deviceId);
			const g = fmtGains(r.restored);
			notifications.add(`Restored Kp ${g.kp}, Ki ${g.ki}, Kd ${g.kd}`, 'success');
			bump('pid');
		} catch (e) {
			notifications.add(`Undo failed: ${errorText(e)}`, 'error');
		} finally {
			undoing = false;
		}
	}

	const shownChanges = $derived(showAllChanges ? changes : changes.slice(0, 5));
	const when = (iso: string) =>
		new Date(iso).toLocaleString([], { dateStyle: 'short', timeStyle: 'short' });
</script>

<div class="rounded-lg border border-border bg-card p-4">
	<div class="mb-3 flex items-center gap-2">
		<History class="h-5 w-5 text-muted-foreground" />
		<h2 class="text-lg font-semibold text-foreground">Tuning history</h2>
	</div>

	{#if !$deviceId}
		<p class="text-sm text-muted-foreground">No device connected.</p>
	{:else}
		<!-- Stored gains -->
		<div class="mb-4 space-y-2">
			<div class="flex flex-wrap items-center justify-between gap-2">
				<h3 class="text-sm font-semibold text-foreground">Stored gains</h3>
				<button
					onclick={undo}
					disabled={!canUndo || undoing}
					title="Restore the gains before the last stored change"
					class="flex items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs font-medium text-foreground hover:bg-accent disabled:opacity-40"
				>
					<Undo2 class="h-3.5 w-3.5" />{undoing ? 'Undoing…' : 'Undo last change'}
				</button>
			</div>
			<p class="text-xs text-muted-foreground">
				In use now: Kp {fmtGains(current).kp} · Ki {fmtGains(current).ki} · Kd {fmtGains(current)
					.kd}
			</p>
			{#if changes.length}
				<div class="overflow-x-auto">
					<table class="w-full min-w-[32rem] text-xs">
						<thead>
							<tr class="text-muted-foreground">
								<th class="pb-1 text-left font-medium">When</th>
								<th class="pb-1 text-left font-medium">Source</th>
								<th class="pb-1 pl-3 text-right font-medium">Kp</th>
								<th class="pb-1 pl-3 text-right font-medium">Ki</th>
								<th class="pb-1 pl-3 text-right font-medium">Kd</th>
								<th class="pb-1 pl-3 text-right font-medium">Before</th>
							</tr>
						</thead>
						<tbody>
							{#each shownChanges as c (c.id)}
								{@const g = fmtGains(c.gains)}
								{@const p = fmtGains(c.previous)}
								<tr
									class="border-t border-border {c.undone
										? 'text-muted-foreground line-through'
										: 'text-foreground'}"
								>
									<td class="whitespace-nowrap py-1 pr-2">{when(c.created_at)}</td>
									<td class="py-1"
										>{sourceLabel(c.source)}{c.rule ? ` · ${ruleLabel(c.rule)}` : ''}</td
									>
									<td class="py-1 pl-3 text-right whitespace-nowrap tabular-nums">{g.kp}</td>
									<td class="py-1 pl-3 text-right whitespace-nowrap tabular-nums">{g.ki}</td>
									<td class="py-1 pl-3 text-right whitespace-nowrap tabular-nums">{g.kd}</td>
									<td
										class="py-1 pl-3 text-right whitespace-nowrap tabular-nums text-muted-foreground"
										>{c.previous ? `${p.kp} / ${p.ki} / ${p.kd}` : '—'}</td
									>
								</tr>
							{/each}
						</tbody>
					</table>
					{#if changes.length > 5}
						<button
							onclick={() => (showAllChanges = !showAllChanges)}
							class="mt-1 text-xs text-muted-foreground hover:text-foreground"
						>
							{showAllChanges ? 'Show fewer' : `Show all ${changes.length}`}
						</button>
					{/if}
				</div>
			{:else}
				<p class="text-xs text-muted-foreground">No stored changes recorded yet.</p>
			{/if}
		</div>

		<!-- Runs -->
		<div class="space-y-2">
			<h3 class="text-sm font-semibold text-foreground">Autotune runs</h3>
			{#if runs.length}
				<div class="overflow-x-auto">
					<table class="w-full min-w-[32rem] text-xs">
						<thead>
							<tr class="text-muted-foreground">
								<th class="pb-1 text-left font-medium">When</th>
								<th class="pb-1 text-left font-medium">Result</th>
								<th class="pb-1 pl-3 text-right font-medium">Fan</th>
								<th class="pb-1 pl-3 text-right font-medium">At</th>
								<th class="pb-1 pl-3 text-right font-medium">Ku</th>
								<th class="pb-1 pl-3 text-right font-medium">Tu</th>
								<th class="pb-1 pl-3 text-right font-medium">Holding</th>
							</tr>
						</thead>
						<tbody>
							{#each runs as r (r.id)}
								{@const q = qualityInfo(r.quality, r.state)}
								<tr
									class="cursor-pointer border-t border-border text-foreground hover:bg-accent {openRun ===
									r.id
										? 'bg-accent'
										: ''}"
									onclick={() => (openRun = openRun === r.id ? null : r.id)}
								>
									<td class="whitespace-nowrap py-1 pr-2">{when(r.created_at)}</td>
									<td class="py-1 {toneText[q.tone]}">{q.label}</td>
									<td class="py-1 pl-3 text-right whitespace-nowrap tabular-nums"
										>{r.fan_pct != null ? `${r.fan_pct.toFixed(0)} %` : '—'}</td
									>
									<td class="py-1 pl-3 text-right whitespace-nowrap tabular-nums"
										>{r.test_temperature != null
											? `${fmtTemp(r.test_temperature, 0)}${tempUnit()}`
											: '—'}</td
									>
									<td class="py-1 pl-3 text-right whitespace-nowrap tabular-nums"
										>{r.ku != null ? r.ku.toFixed(1) : '—'}</td
									>
									<td class="py-1 pl-3 text-right whitespace-nowrap tabular-nums"
										>{r.tu != null ? `${r.tu.toFixed(0)} s` : '—'}</td
									>
									<td class="py-1 pl-3 text-right whitespace-nowrap tabular-nums"
										>{r.bias != null ? `${r.bias.toFixed(0)} %` : '—'}</td
									>
								</tr>
								{#if openRun === r.id}
									<tr>
										<td colspan="7" class="py-2">
											{#if r.source === 'server'}
												<TuneResult run={r} deviceId={$deviceId ?? ''} {current} />
											{:else}
												<AutotuneRunCard run={r} deviceId={$deviceId ?? ''} {current} />
											{/if}
										</td>
									</tr>
								{/if}
							{/each}
						</tbody>
					</table>
				</div>
			{:else}
				<p class="text-xs text-muted-foreground">No autotune runs stored for this roaster yet.</p>
			{/if}
		</div>
	{/if}
</div>
