<script lang="ts">
	/**
	 * PID gains by fan % (docs/autotune-2.md, "Gain schedule"). When on, every fan change
	 * through the dashboard or a replay switches the roaster to the gains for
	 * that fan without storing them; between rows the gains blend linearly.
	 */
	import { SlidersHorizontal, Plus, Trash2 } from '@lucide/svelte';
	import { deviceId, telemetry } from '#lib/stores/telemetry.js';
	import { tuning, type GainSchedule, type ScheduleRow } from '#lib/api/tuning.js';
	import { notifications } from '#lib/stores/notifications.js';
	import {
		scheduleGainsAt,
		fmtGains,
		errorText,
		ruleLabel,
		scheduleProblem,
		cleanRows
	} from '#lib/autotune/tuning.js';
	import { tuningBus } from '#lib/autotune/bus.svelte.js';

	let saved = $state<GainSchedule>({ enabled: false, rows: [] });
	let draft = $state<GainSchedule>({ enabled: false, rows: [] });
	let loaded = $state(false);
	let saving = $state(false);
	let error = $state<string | null>(null);
	/** The server's notes on the saved schedule (D terms on this sensor, rows that disagree). */
	let advice = $state<string[]>([]);

	const fanNow = $derived($telemetry?.fan != null ? Math.round($telemetry.fan) : null);
	const dirty = $derived(JSON.stringify(saved) !== JSON.stringify(draft));
	/** What stops a save: an emptied box sends null, which the server refuses. An empty Kd is 0 (no D). */
	const problem = $derived(scheduleProblem(draft.rows));
	const preview = $derived(fmtGains(scheduleGainsAt({ ...draft, enabled: true }, fanNow)));

	$effect(() => {
		void tuningBus.schedule;
		const id = $deviceId;
		if (!id) return;
		load(id);
	});

	function clone(s: GainSchedule): GainSchedule {
		return { enabled: s.enabled, rows: s.rows.map((r) => ({ ...r })) };
	}

	async function load(id: string) {
		try {
			const v = await tuning.getSchedule(id);
			advice = v.advice ?? [];
			saved = clone(v.schedule);
			draft = clone(v.schedule);
			error = null;
		} catch (e) {
			error = errorText(e);
		} finally {
			loaded = true;
		}
	}

	function addRow() {
		const fan = fanNow ?? 75;
		const g = {
			kp: $telemetry?.Kp ?? 10,
			ki: $telemetry?.Ki ?? 0.1,
			kd: $telemetry?.Kd ?? 0
		};
		const row: ScheduleRow = {
			fan_pct: fan,
			kp: round(g.kp, 3),
			ki: round(g.ki, 5),
			kd: round(g.kd, 3),
			note: 'manual'
		};
		draft.rows = [...draft.rows, row].sort((a, b) => a.fan_pct - b.fan_pct);
	}

	function round(v: number, d: number) {
		const f = 10 ** d;
		return Math.round(v * f) / f;
	}

	/** A gain typed by hand: the row is no longer what its autotune rule gave. */
	function edited(row: ScheduleRow) {
		if (row.rule) {
			row.rule = undefined;
			row.note = 'edited';
		}
	}

	function removeRow(i: number) {
		draft.rows = draft.rows.filter((_, j) => j !== i);
	}

	async function save() {
		if (!$deviceId || problem) return;
		saving = true;
		try {
			const v = await tuning.putSchedule($deviceId, {
				enabled: draft.enabled,
				rows: cleanRows(draft.rows)
			});
			saved = clone(v.schedule);
			draft = clone(v.schedule);
			advice = v.advice ?? [];
			error = null;
			const sent = v.applied ? ` and sent the gains for ${v.fan_pct?.toFixed(0)} % fan` : '';
			notifications.add(`Gain schedule saved${sent}`, 'success');
		} catch (e) {
			error = errorText(e);
		} finally {
			saving = false;
		}
	}

	const cell =
		'w-full rounded border border-border bg-input px-1.5 py-1 text-right text-xs tabular-nums text-foreground';
</script>

<div class="rounded-lg border border-border bg-card p-4">
	<div class="mb-1 flex items-center justify-between gap-2">
		<div class="flex items-center gap-2">
			<SlidersHorizontal class="h-5 w-5 text-violet-400" />
			<h2 class="text-lg font-semibold text-foreground">Gain schedule by fan</h2>
		</div>
		<label class="flex items-center gap-2 text-sm text-foreground">
			<input
				type="checkbox"
				bind:checked={draft.enabled}
				disabled={!$deviceId}
				class="accent-violet-500"
			/>
			On
		</label>
	</div>
	<p class="mb-3 text-sm text-muted-foreground">
		More airflow needs different gains. With the schedule on, changing the fan switches the roaster
		to the gains for that fan (not stored, so the saved gains come back after a restart). Add a row
		from an autotune result at each fan speed you roast at.
	</p>

	{#if !$deviceId}
		<p class="text-sm text-muted-foreground">No device connected.</p>
	{:else if !loaded}
		<p class="text-xs text-muted-foreground">Loading…</p>
	{:else}
		{#if draft.rows.length}
			<div class="overflow-x-auto">
				<table class="w-full text-xs">
					<thead>
						<tr class="text-muted-foreground">
							<th class="pb-1 text-left font-medium">Fan %</th>
							<th class="pb-1 text-right font-medium">Kp</th>
							<th class="pb-1 text-right font-medium">Ki</th>
							<th class="pb-1 text-right font-medium">Kd</th>
							<th
								class="pb-1 pr-3 text-right font-medium"
								title="Heater % that held the test temperature at this fan">Holding</th
							>
							<th class="pb-1 text-left font-medium">From</th>
							<th></th>
						</tr>
					</thead>
					<tbody>
						{#each draft.rows as row, i (i)}
							<tr class="border-t border-border">
								<td class="w-20 py-1 pr-2"
									><input
										type="number"
										min="0"
										max="100"
										step="1"
										bind:value={row.fan_pct}
										class={cell}
										aria-label="Fan %"
									/></td
								>
								<td class="w-20 py-1 pr-2"
									><input
										type="number"
										min="0"
										step="0.1"
										bind:value={row.kp}
										oninput={() => edited(row)}
										class={cell}
										aria-label="Kp"
									/></td
								>
								<td class="w-24 py-1 pr-2"
									><input
										type="number"
										min="0"
										step="0.001"
										bind:value={row.ki}
										oninput={() => edited(row)}
										class={cell}
										aria-label="Ki"
									/></td
								>
								<td class="w-20 py-1 pr-2"
									><input
										type="number"
										min="0"
										step="0.1"
										bind:value={row.kd}
										oninput={() => edited(row)}
										class={cell}
										aria-label="Kd"
									/></td
								>
								<td class="py-1 pr-3 text-right tabular-nums text-muted-foreground"
									>{row.ff != null ? `${row.ff.toFixed(0)} %` : '—'}</td
								>
								<td
									class="py-1 text-muted-foreground"
									title={row.model
										? 'Designed from a model: between two such rows the gains are designed for the fan'
										: undefined}
								>
									{row.rule
										? ruleLabel(row.rule)
										: (row.note ?? (row.run_id ? 'autotune' : ''))}{row.tau_c != null && row.rule
										? ` ${row.tau_c.toFixed(1)}·θ`
										: ''}{row.model ? ' ◆' : ''}
								</td>
								<td class="py-1 text-right">
									<button
										onclick={() => removeRow(i)}
										aria-label="Remove row"
										class="rounded p-1 text-muted-foreground hover:bg-accent hover:text-red-400"
									>
										<Trash2 class="h-3.5 w-3.5" />
									</button>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
			{#if fanNow != null}
				<p class="mt-2 text-xs text-muted-foreground">
					At the current fan ({fanNow} %): Kp {preview.kp} · Ki {preview.ki} · Kd {preview.kd}
				</p>
			{/if}
		{:else}
			<p class="text-xs text-muted-foreground">
				No rows yet. Use “Add to schedule” on an autotune result, or add one by hand.
			</p>
		{/if}
		{#each advice as a (a)}
			<p class="mt-2 border-l-2 border-amber-500 pl-2 text-xs text-amber-400">{a}</p>
		{/each}

		{#if problem && dirty}
			<p class="mt-2 text-xs text-red-400">{problem}</p>
		{/if}
		{#if error}
			<p class="mt-2 text-xs text-red-400">{error}</p>
		{/if}

		<div class="mt-3 flex flex-wrap gap-2">
			<button
				onclick={addRow}
				class="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent"
			>
				<Plus class="h-3.5 w-3.5" /> Add row at current fan
			</button>
			<button
				onclick={save}
				disabled={!dirty || saving || !!problem}
				class="rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-700 disabled:opacity-40"
			>
				{saving ? 'Saving…' : 'Save schedule'}
			</button>
			{#if dirty}
				<button
					onclick={() => (draft = clone(saved))}
					class="rounded-md px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground"
					>Revert</button
				>
			{/if}
		</div>
	{/if}
</div>
