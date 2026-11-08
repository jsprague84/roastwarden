<script lang="ts">
	/**
	 * The designer's table view: every point's time, BT, ET, fan, heater and note as inputs,
	 * in time order; a row selected is the chart's selected point.
	 */
	import { Trash2 } from '@lucide/svelte';
	import {
		indexOfPoint,
		parseTemp,
		showTemp,
		sortPoints,
		type DesignerPoint
	} from '#lib/profile/designer.js';
	import { fromDisplay, roundForInput, tempUnit, toDisplay } from '#lib/stores/units.svelte.js';

	let {
		points = $bindable(),
		selectedIndex = $bindable(),
		ondelete
	}: {
		points: DesignerPoint[];
		selectedIndex: number | null;
		/** The row's delete button, by the point's index in `points`. */
		ondelete: (index: number) => void;
	} = $props();
</script>

{#if points.length === 0}
	<p class="py-4 text-center text-sm text-muted-foreground">
		No points yet. Switch to chart view and click to add points.
	</p>
{:else}
	<div class="overflow-x-auto">
		<table class="w-full text-sm">
			<thead>
				<tr class="border-b border-border text-left text-xs text-muted-foreground">
					<th class="px-3 py-2">Time (s)</th>
					<th class="px-3 py-2">BT ({tempUnit()})</th>
					<th class="px-3 py-2">ET ({tempUnit()})</th>
					<th class="px-3 py-2">Fan %</th>
					<th class="px-3 py-2">Heater %</th>
					<th class="px-3 py-2">Note</th>
					<th class="px-3 py-2 w-10"></th>
				</tr>
			</thead>
			<tbody>
				{#each sortPoints(points) as point, i (i)}
					{@const realIndex = indexOfPoint(points, point)}
					<tr
						class="border-b border-border/50 {selectedIndex === realIndex
							? 'bg-amber-500/10'
							: 'hover:bg-accent'}"
						onclick={() => (selectedIndex = realIndex)}
					>
						<td class="px-3 py-1.5">
							<input
								type="number"
								value={point.time_seconds}
								oninput={(e) => {
									const val = parseInt((e.target as HTMLInputElement).value);
									if (!isNaN(val) && val >= 0) points[realIndex].time_seconds = val;
								}}
								class="w-20 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
								min="0"
							/>
						</td>
						<td class="px-3 py-1.5">
							<input
								type="number"
								value={showTemp(point.target_temp)}
								oninput={(e) => {
									const val = parseFloat((e.target as HTMLInputElement).value);
									if (!isNaN(val) && fromDisplay(val) >= 0)
										points[realIndex].target_temp = parseTemp(val);
								}}
								class="w-20 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
								step="0.1"
								min={roundForInput(toDisplay(0))}
							/>
						</td>
						<td class="px-3 py-1.5">
							<input
								type="number"
								value={showTemp(point.target_env_temp)}
								oninput={(e) => {
									const v = (e.target as HTMLInputElement).value;
									points[realIndex].target_env_temp = v === '' ? null : parseTemp(parseFloat(v));
								}}
								class="w-20 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
								placeholder="-"
								step="0.1"
								min={roundForInput(toDisplay(0))}
							/>
						</td>
						<td class="px-3 py-1.5">
							<input
								type="number"
								value={point.fan_speed ?? ''}
								oninput={(e) => {
									const v = (e.target as HTMLInputElement).value;
									points[realIndex].fan_speed = v === '' ? null : parseInt(v);
								}}
								class="w-20 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
								placeholder="-"
								min="0"
								max="100"
							/>
						</td>
						<td class="px-3 py-1.5">
							<input
								type="number"
								value={point.heater_pct ?? ''}
								oninput={(e) => {
									const v = (e.target as HTMLInputElement).value;
									points[realIndex].heater_pct =
										v === '' ? null : Math.max(0, Math.min(100, parseInt(v)));
								}}
								class="w-20 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
								placeholder="-"
								min="0"
								max="100"
							/>
						</td>
						<td class="px-3 py-1.5">
							<input
								type="text"
								value={point.notes ?? ''}
								oninput={(e) => {
									const v = (e.target as HTMLInputElement).value;
									points[realIndex].notes = v.trim() === '' ? null : v;
								}}
								class="w-32 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
								placeholder="-"
							/>
						</td>
						<td class="px-3 py-1.5">
							<button
								onclick={(e) => {
									e.stopPropagation();
									ondelete(realIndex);
								}}
								class="text-red-400 hover:text-red-300"
								title="Delete point"
							>
								<Trash2 class="h-3.5 w-3.5" />
							</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}
