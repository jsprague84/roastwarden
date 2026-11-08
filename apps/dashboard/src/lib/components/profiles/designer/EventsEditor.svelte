<script lang="ts">
	/**
	 * The profile's events (docs/replay.md): each replayed at a time from CHARGE or when BT
	 * rises through a temperature, as fan, heater, setpoint or mode changes, landmarks and
	 * notes; and the recorded background curve kept or removed.
	 */
	import { Plus, Trash2 } from '@lucide/svelte';
	import type { ProfileCurves, ProfileEventInput } from '#lib/api/profiles.js';
	import { eventLabels } from '#lib/constants/landmarks.js';
	import {
		fmtClock,
		fmtTempLabel,
		parseClock,
		parseTemp,
		setAction,
		setTrigger,
		showTemp
	} from '#lib/profile/designer.js';
	import {
		ACTIONS,
		LANDMARK_TYPES,
		blankEvent,
		eventTimeOnCurve,
		isNumeric
	} from '#lib/profile/events.js';
	import { tempUnit } from '#lib/stores/units.svelte.js';

	let {
		events = $bindable(),
		curve,
		errors,
		curves,
		removeCurves = $bindable()
	}: {
		events: ProfileEventInput[];
		/** The BT curve through the points, °C: where a BT trigger falls in time. */
		curve: [number, number][];
		/** Each event's problem, or null. */
		errors: (string | null)[];
		/** Recorded background curves are read-only here; they can only be kept or removed. */
		curves: ProfileCurves | null;
		removeCurves: boolean;
	} = $props();

	function addEvent() {
		const last = events.at(-1);
		events.push(blankEvent(last?.at_seconds != null ? last.at_seconds + 30 : 0));
	}
</script>

<div class="rounded-lg border border-border bg-card p-4">
	<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
		<h2 class="text-sm font-semibold text-foreground">Events ({events.length})</h2>
		<button
			onclick={addEvent}
			class="inline-flex items-center gap-1 rounded-md border border-border px-2.5 py-1 text-xs text-foreground hover:bg-accent"
		>
			<Plus class="h-3.5 w-3.5" /> Add event
		</button>
	</div>
	{#if curves}
		<div
			class="mb-3 flex flex-wrap items-center gap-2 rounded border border-border/50 bg-card/50 px-3 py-1.5 text-xs text-muted-foreground"
		>
			{#if removeCurves}
				<span>The recorded background curve will be removed on save.</span>
				<button class="text-amber-400 hover:underline" onclick={() => (removeCurves = false)}
					>Keep it</button
				>
			{:else}
				<span>
					Background curve recorded from a roast ({fmtClock(
						(curves.bt.length - 1) * curves.interval_seconds
					)}), shown faint behind the points.
				</span>
				<button class="text-red-400 hover:underline" onclick={() => (removeCurves = true)}
					>Remove</button
				>
			{/if}
		</div>
	{/if}
	{#if events.length === 0}
		<p class="py-2 text-sm text-muted-foreground">
			No events. Add fan, heater, setpoint or mode changes and landmarks to replay at a time or a
			bean temperature.
		</p>
	{:else}
		<div class="overflow-x-auto">
			<table class="w-full text-sm">
				<thead>
					<tr class="border-b border-border text-left text-xs text-muted-foreground">
						<th class="px-2 py-2">Trigger</th>
						<th class="px-2 py-2">At</th>
						<th class="px-2 py-2">Action</th>
						<th class="px-2 py-2">Value</th>
						<th class="px-2 py-2"></th>
						<th class="w-8 px-2 py-2"></th>
					</tr>
				</thead>
				<tbody>
					{#each events as ev, i (ev)}
						{@const err = errors[i]}
						<tr class="border-b border-border/50 align-middle">
							<td class="px-2 py-1.5">
								<select
									value={ev.trigger_kind}
									onchange={(e) =>
										setTrigger(ev, (e.target as HTMLSelectElement).value as 'time' | 'bt', curve)}
									class="rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
									aria-label="Trigger"
								>
									<option value="time">Time</option>
									<option value="bt">BT</option>
								</select>
							</td>
							<td class="px-2 py-1.5 whitespace-nowrap">
								{#if ev.trigger_kind === 'time'}
									<input
										type="text"
										inputmode="numeric"
										value={ev.at_seconds == null ? '' : fmtClock(ev.at_seconds)}
										onchange={(e) =>
											(ev.at_seconds = parseClock((e.target as HTMLInputElement).value))}
										class="w-20 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
										placeholder="m:ss"
										aria-label="Time from charge"
									/>
								{:else}
									<input
										type="number"
										value={showTemp(ev.at_bt)}
										oninput={(e) => {
											const v = (e.target as HTMLInputElement).value;
											ev.at_bt = v === '' ? null : parseTemp(parseFloat(v));
										}}
										class="w-20 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
										step="0.5"
										aria-label="Bean temperature"
									/>
									<span class="text-xs text-muted-foreground">{tempUnit()}</span>
								{/if}
							</td>
							<td class="px-2 py-1.5">
								<select
									value={ev.action}
									onchange={(e) =>
										setAction(
											ev,
											(e.target as HTMLSelectElement).value as ProfileEventInput['action']
										)}
									class="rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
									aria-label="Action"
								>
									{#each ACTIONS as a (a.value)}
										<option value={a.value}>{a.label}</option>
									{/each}
								</select>
							</td>
							<td class="px-2 py-1.5">
								{#if ev.action === 'setpoint'}
									<input
										type="number"
										value={showTemp(ev.value)}
										oninput={(e) => {
											const v = (e.target as HTMLInputElement).value;
											ev.value = v === '' ? null : parseTemp(parseFloat(v));
										}}
										class="w-20 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
										aria-label="Setpoint"
									/>
									<span class="text-xs text-muted-foreground">{tempUnit()}</span>
								{:else if isNumeric(ev.action)}
									<input
										type="number"
										value={ev.value ?? ''}
										oninput={(e) => {
											const v = (e.target as HTMLInputElement).value;
											ev.value = v === '' ? null : parseFloat(v);
										}}
										class="w-20 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
										min="0"
										max="100"
										aria-label="Percent"
									/>
									<span class="text-xs text-muted-foreground">%</span>
								{:else if ev.action === 'mode'}
									<select
										bind:value={ev.label}
										class="rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
										aria-label="Mode"
									>
										<option value="auto">Auto (PID)</option>
										<option value="manual">Manual</option>
									</select>
								{:else if ev.action === 'landmark'}
									<select
										bind:value={ev.label}
										class="rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
										aria-label="Landmark"
									>
										{#each LANDMARK_TYPES as t (t)}
											<option value={t}>{eventLabels[t] ?? t}</option>
										{/each}
									</select>
								{:else}
									<input
										type="text"
										bind:value={ev.label}
										class="w-40 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
										placeholder="Note text"
										aria-label="Note"
									/>
								{/if}
							</td>
							<td class="px-2 py-1.5 text-xs {err ? 'text-red-400' : 'text-muted-foreground'}">
								{#if err}
									{err}
								{:else if ev.trigger_kind === 'bt'}
									{@const t = eventTimeOnCurve(ev, curve)}
									{t == null ? 'not reached on this curve' : `≈ ${fmtClock(t)}`}
								{:else if ev.at_bt != null}
									≈ {fmtTempLabel(ev.at_bt)}
								{/if}
							</td>
							<td class="px-2 py-1.5">
								<button
									onclick={() => events.splice(i, 1)}
									class="text-red-400 hover:text-red-300"
									title="Delete event"
								>
									<Trash2 class="h-3.5 w-3.5" />
								</button>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		<p class="mt-2 text-xs text-muted-foreground">
			Time is from CHARGE. A BT trigger fires once as the beans heat through that temperature, after
			the turning point. The replay engine can run a profile by either.
		</p>
	{/if}
</div>
