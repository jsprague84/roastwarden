<script lang="ts">
	import { CHART_FONT } from '#lib/constants/chartTheme.js';
	import { onMount, onDestroy } from 'svelte';
	import * as echarts from 'echarts/core';
	import { LineChart, CustomChart } from 'echarts/charts';
	import {
		GridComponent,
		TitleComponent,
		TooltipComponent,
		LegendComponent,
		MarkLineComponent,
		MarkAreaComponent,
		MarkPointComponent,
		GraphicComponent
	} from 'echarts/components';
	import { CanvasRenderer } from 'echarts/renderers';
	import type { ECharts, ComposeOption } from 'echarts/core';
	import type { LineSeriesOption, CustomSeriesOption } from 'echarts/charts';
	import type {
		GridComponentOption,
		TitleComponentOption,
		TooltipComponentOption,
		LegendComponentOption,
		MarkLineComponentOption,
		MarkAreaComponentOption,
		MarkPointComponentOption,
		GraphicComponentOption
	} from 'echarts/components';

	export type ECOption = ComposeOption<
		| LineSeriesOption
		| CustomSeriesOption
		| GridComponentOption
		| TitleComponentOption
		| TooltipComponentOption
		| LegendComponentOption
		| MarkLineComponentOption
		| MarkAreaComponentOption
		| MarkPointComponentOption
		| GraphicComponentOption
	>;

	echarts.use([
		LineChart,
		CustomChart,
		GridComponent,
		TitleComponent,
		TooltipComponent,
		LegendComponent,
		MarkLineComponent,
		MarkAreaComponent,
		MarkPointComponent,
		GraphicComponent,
		CanvasRenderer
	]);

	let { option = $bindable<ECOption>({}) }: { option: ECOption } = $props();

	/** Every chart in the app's face, unless the option sets its own. */
	const withFont = (o: ECOption): ECOption => ({
		...o,
		textStyle: { fontFamily: CHART_FONT, ...((o as { textStyle?: object }).textStyle ?? {}) }
	});

	let container: HTMLDivElement;
	let chart: ECharts | null = null;
	let resizeObserver: ResizeObserver | null = null;

	onMount(() => {
		chart = echarts.init(container);
		chart.setOption(withFont(option));

		resizeObserver = new ResizeObserver(() => {
			chart?.resize();
		});
		resizeObserver.observe(container);
	});

	onDestroy(() => {
		resizeObserver?.disconnect();
		chart?.dispose();
		chart = null;
	});

	$effect(() => {
		if (chart && option) {
			chart.setOption(withFont(option), { replaceMerge: ['series', 'graphic'] });
		}
	});

	export function getInstance(): ECharts | null {
		return chart;
	}
</script>

<div bind:this={container} class="h-full w-full" style="min-height: 300px;"></div>
