<script lang="ts">
	/** The selected point's fields under the chart: time, BT, ET, fan and heater. */
	import { parseTemp, showTemp, type DesignerPoint } from '#lib/profile/designer.js';
	import { fromDisplay, roundForInput, tempUnit, toDisplay } from '#lib/stores/units.svelte.js';

	let {
		points = $bindable(),
		index
	}: {
		points: DesignerPoint[];
		/** The selected point's index in `points`; nothing is shown without one. */
		index: number | null;
	} = $props();
</script>

{#if index !== null && index >= 0 && index < points.length}
	{@const point = points[index]}
	<div class="rounded-lg border border-amber-500/30 bg-amber-500/10 p-4">
		<h3 class="mb-2 text-sm font-semibold text-amber-400">Selected Point</h3>
		<div class="grid gap-3 sm:grid-cols-5">
			<div>
				<label for="edit-time" class="mb-1 block text-xs font-medium text-muted-foreground"
					>Time (seconds)</label
				>
				<input
					id="edit-time"
					type="number"
					bind:value={point.time_seconds}
					class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
					min="0"
				/>
			</div>
			<div>
				<label for="edit-temp" class="mb-1 block text-xs font-medium text-muted-foreground"
					>BT ({tempUnit()})</label
				>
				<input
					id="edit-temp"
					type="number"
					value={showTemp(point.target_temp)}
					oninput={(e) => {
						const val = parseFloat((e.target as HTMLInputElement).value);
						if (!isNaN(val) && fromDisplay(val) >= 0) point.target_temp = parseTemp(val);
					}}
					class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
					step="0.1"
					min={roundForInput(toDisplay(0))}
				/>
			</div>
			<div>
				<label for="edit-et" class="mb-1 block text-xs font-medium text-muted-foreground"
					>ET ({tempUnit()})</label
				>
				<input
					id="edit-et"
					type="number"
					value={showTemp(point.target_env_temp)}
					oninput={(e) => {
						const v = (e.target as HTMLInputElement).value;
						point.target_env_temp = v === '' ? null : parseTemp(parseFloat(v));
					}}
					class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
					placeholder="Optional"
					step="0.1"
					min={roundForInput(toDisplay(0))}
				/>
			</div>
			<div>
				<label for="edit-fan" class="mb-1 block text-xs font-medium text-muted-foreground"
					>Fan (%)</label
				>
				<input
					id="edit-fan"
					type="number"
					value={point.fan_speed ?? ''}
					oninput={(e) => {
						const v = (e.target as HTMLInputElement).value;
						point.fan_speed = v === '' ? null : parseInt(v);
					}}
					class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
					placeholder="Optional"
					min="0"
					max="100"
				/>
			</div>
			<div>
				<label for="edit-heater" class="mb-1 block text-xs font-medium text-muted-foreground"
					>Heater (%)</label
				>
				<input
					id="edit-heater"
					type="number"
					value={point.heater_pct ?? ''}
					oninput={(e) => {
						const v = (e.target as HTMLInputElement).value;
						point.heater_pct = v === '' ? null : Math.max(0, Math.min(100, parseInt(v)));
					}}
					class="w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
					placeholder="Optional"
					min="0"
					max="100"
				/>
			</div>
		</div>
	</div>
{/if}
