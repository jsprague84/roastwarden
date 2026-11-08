<script
	lang="ts"
	generics="P extends { time_seconds: number; target_temp: number; fan_speed: number | null; target_env_temp: number | null }"
>
	import { chartPalette } from '#lib/constants/chartTheme.js';
	import Chart, { type ECOption } from '#lib/components/ui/Chart.svelte';
	import {
		fitNaturalCubicSpline,
		evaluateSplineGrid,
		evaluateDerivativeGrid
	} from '#lib/utils/spline.js';
	import {
		toDisplay,
		deltaToDisplay,
		deltaFromDisplay,
		tempUnit,
		rorUnit
	} from '#lib/stores/units.svelte.js';

	const pal = $derived(chartPalette());

	const {
		points,
		onApply,
		onCancel
	}: {
		points: P[];
		/** Transposed points (other fields kept) and the time scale applied. */
		onApply: (transformed: P[], timeScale: number) => void;
		onCancel: () => void;
	} = $props();

	let timeScale = $state(1.0);
	// Slider works in the display unit; the applied shift is a °C difference.
	let tempShiftDisp = $state(0);
	let tempShift = $derived(deltaFromDisplay(tempShiftDisp));
	let shiftLimit = $derived(Math.round(deltaToDisplay(30)));
	let tempScale = $state(1.0);

	// Compute transposed points
	let transposed = $derived.by(() => {
		const sorted = [...points].sort((a, b) => a.time_seconds - b.time_seconds);
		const meanTemp =
			sorted.length > 0 ? sorted.reduce((s, p) => s + p.target_temp, 0) / sorted.length : 0;

		return sorted.map((p) => {
			const newTime = Math.max(1, Math.round(p.time_seconds * timeScale));
			const scaledTemp = meanTemp + (p.target_temp - meanTemp) * tempScale;
			const newTemp = Math.round((scaledTemp + tempShift) * 10) / 10;

			// Apply same transforms to ET if present
			let newEt: number | null = null;
			if (p.target_env_temp !== null) {
				const scaledEt = meanTemp + (p.target_env_temp - meanTemp) * tempScale;
				newEt = Math.max(0, Math.round((scaledEt + tempShift) * 10) / 10);
			}

			return {
				...p,
				time_seconds: newTime,
				target_temp: Math.max(0, newTemp),
				target_env_temp: newEt
			};
		});
	});

	// Chart option for preview
	let chartOption = $derived.by<ECOption>(() => {
		const original = [...points].sort((a, b) => a.time_seconds - b.time_seconds);
		// Preview is drawn in the display unit; `points`/`transposed` stay °C.
		const originalData = original.map((p) => [p.time_seconds, toDisplay(p.target_temp)]);

		// Transposed BT spline
		const maxTime = transposed.length > 0 ? transposed[transposed.length - 1].time_seconds : 0;
		const btSpline = fitNaturalCubicSpline(
			transposed.map((p) => ({ x: p.time_seconds, y: p.target_temp }))
		);
		const transposedData: [number, number][] = (
			transposed.length >= 2
				? evaluateSplineGrid(btSpline, 0, maxTime, 2)
				: transposed.map((p): [number, number] => [p.time_seconds, p.target_temp])
		).map(([t, y]) => [t, toDisplay(y)]);

		// Transposed RoR from spline derivative
		const rorData: [number, number][] =
			transposed.length >= 2
				? evaluateDerivativeGrid(btSpline, 0, maxTime, 5).map(([t, dy]) => [
						t,
						Math.round(deltaToDisplay(dy * 60) * 10) / 10
					])
				: [];

		// ET spline (transposed)
		const etPoints = transposed.filter((p) => p.target_env_temp !== null);
		const hasEtData = etPoints.length >= 2;
		let etCurveData: [number, number][] = [];
		if (hasEtData) {
			const etSpline = fitNaturalCubicSpline(
				etPoints.map((p) => ({ x: p.time_seconds, y: p.target_env_temp! }))
			);
			etCurveData = evaluateSplineGrid(etSpline, 0, maxTime, 2).map(([t, y]) => [t, toDisplay(y)]);
		}

		return {
			animation: false,
			grid: { left: 50, right: 50, top: 35, bottom: 35 },
			legend: {
				show: true,
				top: 5,
				textStyle: { color: pal.axisText, fontSize: 10 }
			},
			tooltip: { trigger: 'axis' },
			xAxis: {
				type: 'value',
				name: 'Time (s)',
				nameTextStyle: { color: pal.axisText },
				axisLabel: {
					color: pal.axisText,
					formatter: (val: number) => {
						const m = Math.floor(val / 60);
						const s = Math.round(val % 60);
						return `${m}:${s.toString().padStart(2, '0')}`;
					}
				},
				axisLine: { lineStyle: { color: pal.line } },
				splitLine: { show: false }
			},
			yAxis: [
				{
					type: 'value',
					name: `Temp (${tempUnit()})`,
					nameTextStyle: { color: pal.axisText },
					axisLabel: { color: pal.axisText },
					axisLine: { lineStyle: { color: pal.line } },
					splitLine: { lineStyle: { color: pal.lineFaint } }
				},
				{
					type: 'value',
					name: `RoR (${rorUnit()})`,
					nameTextStyle: { color: '#34d399' },
					axisLabel: { color: '#34d399' },
					axisLine: { lineStyle: { color: '#34d399' } },
					splitLine: { show: false }
				}
			],
			series: [
				{
					name: 'Original',
					type: 'line',
					data: originalData,
					itemStyle: { color: pal.mutedText },
					lineStyle: { width: 1.5, type: 'dashed', color: pal.mutedText },
					showSymbol: false
				},
				{
					name: 'Transposed',
					type: 'line',
					data: transposedData,
					itemStyle: { color: '#f59e0b' },
					lineStyle: { width: 2, color: '#f59e0b' },
					showSymbol: false
				},
				...(hasEtData
					? [
							{
								name: 'Transposed ET',
								type: 'line' as const,
								data: etCurveData,
								itemStyle: { color: '#60a5fa' },
								lineStyle: { width: 1.5, type: 'dashed' as const, color: '#60a5fa' },
								showSymbol: false,
								silent: true
							}
						]
					: []),
				{
					name: 'Transposed RoR',
					type: 'line',
					data: rorData,
					yAxisIndex: 1,
					itemStyle: { color: '#34d399' },
					lineStyle: { width: 1, type: 'dashed', color: '#34d399' },
					showSymbol: false,
					silent: true
				}
			]
		};
	});

	function handleApply() {
		onApply(transposed, timeScale);
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
	class="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
	onclick={(e) => {
		if (e.target === e.currentTarget) onCancel();
	}}
>
	<div class="w-full max-w-2xl rounded-lg border border-border bg-card p-5 shadow-xl">
		<h2 class="mb-4 text-lg font-semibold text-foreground">Transpose Profile</h2>

		<!-- Controls -->
		<div class="mb-4 grid gap-3 sm:grid-cols-3">
			<div>
				<label for="time-scale" class="mb-1 block text-xs font-medium text-muted-foreground">
					Time Scale ({timeScale.toFixed(2)}x)
				</label>
				<input
					id="time-scale"
					type="range"
					min="0.5"
					max="2.0"
					step="0.05"
					bind:value={timeScale}
					class="w-full accent-action"
				/>
				<div class="mt-0.5 flex justify-between text-[10px] text-muted-foreground">
					<span>0.5x (faster)</span>
					<span>2x (slower)</span>
				</div>
			</div>
			<div>
				<label for="temp-shift" class="mb-1 block text-xs font-medium text-muted-foreground">
					Temp Shift ({tempShiftDisp > 0 ? '+' : ''}{tempShiftDisp}{tempUnit()})
				</label>
				<input
					id="temp-shift"
					type="range"
					min={-shiftLimit}
					max={shiftLimit}
					step="1"
					bind:value={tempShiftDisp}
					class="w-full accent-action"
				/>
				<div class="mt-0.5 flex justify-between text-[10px] text-muted-foreground">
					<span>-{shiftLimit}{tempUnit()}</span>
					<span>+{shiftLimit}{tempUnit()}</span>
				</div>
			</div>
			<div>
				<label for="temp-scale" class="mb-1 block text-xs font-medium text-muted-foreground">
					Temp Scale ({tempScale.toFixed(2)}x)
				</label>
				<input
					id="temp-scale"
					type="range"
					min="0.7"
					max="1.3"
					step="0.05"
					bind:value={tempScale}
					class="w-full accent-action"
				/>
				<div class="mt-0.5 flex justify-between text-[10px] text-muted-foreground">
					<span>0.7x (less spread)</span>
					<span>1.3x (more spread)</span>
				</div>
			</div>
		</div>

		<!-- Preview chart -->
		<div style="height: 300px;" class="mb-4">
			<Chart option={chartOption} />
		</div>

		<!-- Actions -->
		<div class="flex justify-end gap-2">
			<button
				onclick={onCancel}
				class="rounded-md border border-border px-4 py-2 text-sm font-medium text-foreground hover:bg-accent"
			>
				Cancel
			</button>
			<button
				onclick={handleApply}
				class="rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover"
			>
				Apply
			</button>
		</div>
	</div>
</div>
