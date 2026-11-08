<script lang="ts">
	// A roast's cupping on the classic SCA form (docs/cupping.md): seven quality scales, three
	// attributes scored cup by cup, defects, and the final score.
	import { chartPalette } from '#lib/constants/chartTheme.js';
	import * as echarts from 'echarts/core';
	import { RadarChart } from 'echarts/charts';
	import { TooltipComponent, TitleComponent } from 'echarts/components';
	import { CanvasRenderer } from 'echarts/renderers';
	import { cupping, type CreateCuppingRequest } from '#lib/api/cupping.js';
	import { notifications } from '#lib/stores/notifications.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { HELP } from '#lib/constants/help.js';
	import {
		ATTRIBUTES,
		CUPS,
		capDefects,
		cupsFromScore,
		freshScores,
		scoreFromCups,
		summarize
	} from '#lib/cupping/cupping.js';

	const pal = $derived(chartPalette());

	echarts.use([RadarChart, TooltipComponent, TitleComponent, CanvasRenderer]);

	const QUALITY = ATTRIBUTES.filter((a) => a.kind === 'quality');
	const BY_CUP = ATTRIBUTES.filter((a) => a.kind === 'cups');

	let { sessionId }: { sessionId: string } = $props();

	let scores = $state<Record<string, number>>(freshScores());
	let taint = $state(0);
	let fault = $state(0);
	let notes = $state('');
	let saving = $state(false);
	let hasExisting = $state(false);

	const summary = $derived(summarize(scores, taint, fault));

	let chartContainer: HTMLDivElement;
	let chart: echarts.ECharts | null = null;

	// Initialize chart and load data
	$effect(() => {
		if (!chartContainer) return;
		chart = echarts.init(chartContainer);
		updateChart();

		cupping
			.get(sessionId)
			.then((existing) => {
				if (existing) {
					hasExisting = true;
					notes = existing.notes ?? '';
					taint = existing.taint_cups ?? 0;
					fault = existing.fault_cups ?? 0;
					for (const attr of existing.attributes) {
						if (attr.attribute_name in scores) scores[attr.attribute_name] = attr.score;
					}
				}
			})
			.catch(() => {});

		return () => {
			chart?.dispose();
			chart = null;
		};
	});

	function updateChart() {
		if (!chart) return;
		chart.setOption({
			animation: false,
			radar: {
				indicator: ATTRIBUTES.map((a) => ({ name: a.label, max: 10, min: 0 })),
				shape: 'polygon' as const,
				splitNumber: 5,
				axisName: { color: pal.axisText, fontSize: 11 },
				splitLine: { lineStyle: { color: pal.lineFaint } },
				splitArea: { show: false },
				axisLine: { lineStyle: { color: pal.line } }
			},
			series: [
				{
					type: 'radar',
					data: [
						{
							value: ATTRIBUTES.map((a) => scores[a.key] ?? 0),
							name: 'Cupping',
							lineStyle: { color: '#f59e0b', width: 2 },
							itemStyle: { color: '#f59e0b' },
							areaStyle: { color: 'rgba(245, 158, 11, 0.15)' }
						}
					],
					symbol: 'circle',
					symbolSize: 6
				}
			]
		});
	}

	$effect(() => {
		ATTRIBUTES.forEach((a) => scores[a.key]);
		updateChart();
	});

	function toggleCup(key: string, cup: number) {
		const passing = cupsFromScore(scores[key]);
		// Ticking cup n means cups 1..n pass; clicking the last passing cup takes it away
		scores[key] = scoreFromCups(cup + 1 === passing ? cup : cup + 1);
	}

	function setDefects(which: 'taint' | 'fault', n: number) {
		const next = capDefects(which, which === 'taint' ? n : taint, which === 'fault' ? n : fault);
		taint = next.taint;
		fault = next.fault;
	}

	async function handleSave() {
		saving = true;
		try {
			const req: CreateCuppingRequest = {
				scoring_framework: 'sca',
				notes: notes || undefined,
				attributes: ATTRIBUTES.map((a) => ({ name: a.key, score: scores[a.key] ?? 0 })),
				taint_cups: taint,
				fault_cups: fault
			};
			await cupping.create(sessionId, req);
			hasExisting = true;
			notifications.add('Cupping saved', 'success');
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			notifications.add(`Failed to save cupping: ${msg}`, 'error');
		} finally {
			saving = false;
		}
	}

	async function handleDelete() {
		if (!confirm('Delete this cupping? This cannot be undone.')) return;
		try {
			await cupping.delete(sessionId);
			hasExisting = false;
			scores = freshScores();
			taint = 0;
			fault = 0;
			notes = '';
			notifications.add('Cupping deleted', 'success');
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			notifications.add(`Failed to delete cupping: ${msg}`, 'error');
		}
	}

	const fmt = (n: number | null) => (n == null ? '–' : n.toFixed(2));
</script>

<div class="space-y-4">
	<div class="grid gap-4 md:grid-cols-2">
		<div class="flex items-center justify-center">
			<div bind:this={chartContainer} class="h-72 w-full" style="min-height: 288px;"></div>
		</div>

		<div class="space-y-3">
			<div class="space-y-1.5">
				<div class="flex items-center gap-1.5 text-xs text-muted-foreground">
					Quality <Hint inline help={HELP.cupQuality} />
				</div>
				{#each QUALITY as attr (attr.key)}
					<div class="flex items-center gap-3">
						<label for="cupping-{attr.key}" class="w-32 text-xs font-medium text-muted-foreground"
							>{attr.label}</label
						>
						<input
							id="cupping-{attr.key}"
							type="range"
							min={Math.min(6, scores[attr.key])}
							max="10"
							step="0.25"
							bind:value={scores[attr.key]}
							class="flex-1 accent-action"
						/>
						<span class="w-12 shrink-0 text-right font-mono text-sm tabular-nums text-foreground"
							>{scores[attr.key].toFixed(2)}</span
						>
					</div>
				{/each}
			</div>

			<div class="space-y-1.5">
				<div class="flex items-center gap-1.5 text-xs text-muted-foreground">
					Cup by cup <Hint inline help={HELP.cupCups} />
				</div>
				{#each BY_CUP as attr (attr.key)}
					{@const passing = cupsFromScore(scores[attr.key])}
					<div class="flex items-center gap-3">
						<span
							class="w-32 text-xs font-medium text-muted-foreground"
							id="cupping-{attr.key}-label">{attr.label}</span
						>
						<div
							class="flex flex-1 gap-1.5"
							role="group"
							aria-labelledby="cupping-{attr.key}-label"
						>
							{#each Array(CUPS) as _, i (i)}
								<button
									type="button"
									class="h-6 w-7 rounded-sm border text-[11px] tabular-nums {i < passing
										? 'border-action bg-action text-action-foreground'
										: 'border-border text-muted-foreground hover:border-focus'}"
									aria-pressed={i < passing}
									aria-label="{attr.label}: cup {i + 1} {i < passing ? 'passes' : 'fails'}"
									onclick={() => toggleCup(attr.key, i)}>{i + 1}</button
								>
							{/each}
						</div>
						<span class="w-12 shrink-0 text-right font-mono text-sm tabular-nums text-foreground"
							>{scores[attr.key].toFixed(2)}</span
						>
					</div>
				{/each}
			</div>

			<div class="space-y-1.5">
				<div class="flex items-center gap-1.5 text-xs text-muted-foreground">
					Defects <Hint inline help={HELP.cupDefects} />
				</div>
				{#each [{ which: 'taint' as const, label: 'Taint · 2 a cup', n: taint }, { which: 'fault' as const, label: 'Fault · 4 a cup', n: fault }] as d (d.which)}
					<div class="flex items-center gap-3">
						<span
							class="w-32 text-xs font-medium text-muted-foreground"
							id="cupping-{d.which}-label">{d.label}</span
						>
						<div
							class="flex flex-1 gap-1.5"
							role="radiogroup"
							aria-labelledby="cupping-{d.which}-label"
						>
							{#each Array(CUPS + 1) as _, n (n)}
								<button
									type="button"
									role="radio"
									aria-checked={d.n === n}
									class="h-6 w-7 rounded-sm border text-[11px] tabular-nums {d.n === n
										? 'border-selected bg-selected text-selected-foreground'
										: 'border-border text-muted-foreground hover:border-focus'}"
									onclick={() => setDefects(d.which, n)}>{n}</button
								>
							{/each}
						</div>
						<span class="w-12 shrink-0 text-right font-mono text-sm tabular-nums text-foreground"
							>{d.n ? `−${d.n * (d.which === 'taint' ? 2 : 4)}` : '0'}</span
						>
					</div>
				{/each}
			</div>

			<div
				class="flex flex-wrap items-baseline justify-between gap-x-3 border-t border-border pt-2"
				data-testid="cupping-summary"
			>
				<span class="text-xs text-muted-foreground tabular-nums">
					Total {fmt(summary.total)} − defects {summary.defects}
				</span>
				<span class="text-sm text-muted-foreground">
					Final <span class="text-lg font-bold text-highlight tabular-nums"
						>{fmt(summary.final)}</span
					>
					{#if summary.band}<span class="ml-1">{summary.band}</span>{/if}
				</span>
			</div>
		</div>
	</div>

	<div>
		<label for="cupping-notes" class="block text-sm font-medium text-foreground"
			>Tasting Notes</label
		>
		<textarea
			id="cupping-notes"
			bind:value={notes}
			rows={3}
			placeholder="Describe flavors, aromas, and impressions..."
			class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus"
		></textarea>
	</div>

	<div class="flex items-center gap-2">
		<button
			onclick={handleSave}
			disabled={saving}
			class="rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
		>
			{saving ? 'Saving...' : hasExisting ? 'Update Scores' : 'Save Scores'}
		</button>
		{#if hasExisting}
			<button
				onclick={handleDelete}
				class="rounded-md border border-border px-4 py-2 text-sm font-medium text-red-400 hover:bg-red-500/15"
			>
				Delete
			</button>
		{/if}
	</div>
</div>
