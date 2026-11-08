<script lang="ts">
	/**
	 * A profile drawn the way Artisan draws a background: BT and ET, their
	 * rates of rise, fan and heater on a % axis, landmark lines with time and
	 * BT, the phases as bands, and control events as markers on the BT curve.
	 * The roast it was recorded from, if any, is drawn faint behind it.
	 */
	import Chart, { type ECOption } from '#lib/components/ui/Chart.svelte';
	import { chartPalette } from '#lib/constants/chartTheme.js';
	import {
		toDisplay,
		deltaToDisplay,
		tempUnit,
		rorUnit,
		fmtTemp
	} from '#lib/stores/units.svelte.js';
	import { fmtClock, type Num } from '#lib/scope/scopeData.js';
	import { controlLabel, type ProfileView } from '#lib/profile/view.js';

	let { view }: { view: ProfileView } = $props();

	const pal = $derived(chartPalette());

	const PHASE_COLOR: Record<string, string> = {
		drying: 'rgba(132, 204, 22, 0.07)',
		maillard: 'rgba(245, 158, 11, 0.08)',
		development: 'rgba(239, 68, 68, 0.08)'
	};
	const SHORT: Record<string, string> = {
		CHARGE: 'CHARGE',
		TP: 'TP',
		DRY: 'DRY',
		FCs: 'FCs',
		FCe: 'FCe',
		SCs: 'SCs',
		SCe: 'SCe',
		DROP: 'DROP',
		COOL: 'COOL'
	};

	function pairs(t: number[], v: Num[], conv: (x: number) => number): [number, number | null][] {
		return t.map((s, i) => [s, v[i] == null ? null : Math.round(conv(v[i] as number) * 10) / 10]);
	}

	const option = $derived.by<ECOption>(() => {
		const v = view;
		const temp = (c: number) => toDisplay(c);
		const end = v.t.length ? v.t[v.t.length - 1] : 600;
		const dry = v.landmarks.find((l) => l.code === 'DRY');
		const fc = v.landmarks.find((l) => l.code === 'FCs');
		const drop = v.landmarks.find((l) => l.code === 'DROP');
		const bands: [number, number, string, string][] = [];
		if (dry) bands.push([0, dry.t, 'drying', 'Drying']);
		if (dry && fc) bands.push([dry.t, fc.t, 'maillard', 'Maillard']);
		if (fc && drop) bands.push([fc.t, drop.t, 'development', 'Development']);

		const line = (
			name: string,
			data: [number, number | null][],
			color: string,
			extra: Record<string, unknown> = {}
		) => ({
			name,
			type: 'line' as const,
			data,
			showSymbol: false,
			connectNulls: false,
			itemStyle: { color },
			lineStyle: { color, width: 1.5 },
			emphasis: { disabled: true },
			...extra
		});

		const series: ECOption['series'] = [];
		if (v.recorded) {
			series.push(
				line('Recorded BT', pairs(v.recorded.t, v.recorded.bt, temp), '#f59e0b', {
					lineStyle: { color: '#f59e0b', width: 4, opacity: 0.15 },
					silent: true,
					tooltip: { show: false }
				})
			);
			if (v.recorded.et.some((x) => x != null)) {
				series.push(
					line('Recorded ET', pairs(v.recorded.t, v.recorded.et, temp), '#3b82f6', {
						lineStyle: { color: '#3b82f6', width: 4, opacity: 0.12 },
						silent: true,
						tooltip: { show: false }
					})
				);
			}
		}
		series.push(
			line('BT', pairs(v.t, v.bt, temp), '#f59e0b', {
				lineStyle: { color: '#f59e0b', width: 2.5 },
				z: 5,
				markLine: {
					silent: true,
					symbol: 'none',
					lineStyle: { color: pal.axisText, type: 'dashed', width: 1, opacity: 0.6 },
					label: {
						color: pal.labelText,
						fontSize: 10,
						position: 'insideEndTop',
						formatter: (p: { name: string }) => p.name
					},
					data: v.landmarks
						.filter((l) => l.code !== 'CHARGE')
						.map((l) => ({
							name: `${SHORT[l.code]} ${fmtClock(l.t)}${l.bt != null ? ` ${fmtTemp(l.bt, 0)}°` : ''}`,
							xAxis: l.t
						}))
				},
				markArea: {
					silent: true,
					label: { color: pal.mutedText, fontSize: 10, position: 'insideBottom' },
					data: bands.map(([a, b, key, label]) => [
						{ xAxis: a, name: label, itemStyle: { color: PHASE_COLOR[key] } },
						{ xAxis: b }
					])
				},
				markPoint: {
					symbol: 'triangle',
					symbolSize: 9,
					symbolRotate: 180,
					symbolOffset: [0, -10],
					itemStyle: { color: pal.axisText },
					label: {
						show: true,
						position: 'top',
						fontSize: 10,
						color: pal.labelText,
						distance: 4,
						formatter: (p: { name: string }) => p.name
					},
					data: v.controls
						.filter((c) => c.action === 'setpoint' || c.action === 'mode' || c.action === 'note')
						.map((c) => {
							const y =
								v.bt[
									Math.min(
										v.bt.length - 1,
										Math.max(
											0,
											v.t.findIndex((s) => s >= c.t)
										)
									)
								];
							return {
								name: controlLabel(c, (x) => `${fmtTemp(x, 0)}${tempUnit()}`),
								coord: [c.t, y == null ? null : temp(y)]
							};
						})
				}
			})
		);
		if (v.hasEt) series.push(line('ET', pairs(v.t, v.et, temp), '#3b82f6'));
		series.push(line('ΔBT', pairs(v.t, v.btRor, deltaToDisplay), '#10b981', { yAxisIndex: 1 }));
		if (v.hasEt)
			series.push(
				line('ΔET', pairs(v.t, v.etRor, deltaToDisplay), '#38bdf8', {
					yAxisIndex: 1,
					lineStyle: { color: '#38bdf8', width: 1, type: 'dashed' }
				})
			);
		if (v.hasFan)
			series.push(
				line(
					'Fan',
					pairs(v.t, v.fan, (x) => x),
					'#a78bfa',
					{ yAxisIndex: 2, step: 'end', lineStyle: { color: '#a78bfa', width: 1.25 } }
				)
			);
		if (v.hasHeater)
			series.push(
				line(
					'Heater',
					pairs(v.t, v.heater, (x) => x),
					'#f87171',
					{ yAxisIndex: 2, step: 'end', lineStyle: { color: '#f87171', width: 1.25 } }
				)
			);

		// Fan and heater events (time-placed) as short ticks on the % axis.
		const pctEvents = v.controls.filter(
			(c) => (c.action === 'fan' || c.action === 'heater') && c.event.value != null
		);
		if (pctEvents.length) {
			series.push({
				name: 'Events',
				type: 'line',
				data: [],
				yAxisIndex: 2,
				markPoint: {
					symbol: 'circle',
					symbolSize: 6,
					label: {
						show: true,
						position: 'right',
						fontSize: 10,
						color: pal.labelText,
						formatter: (p: { name: string }) => p.name
					},
					data: pctEvents.map((c) => ({
						name: controlLabel(c, (x) => `${x}`),
						coord: [c.t, c.event.value as number],
						itemStyle: { color: c.action === 'fan' ? '#a78bfa' : '#f87171' }
					}))
				}
			});
		}

		const axisLabel = { color: pal.axisText, fontSize: 11 };
		return {
			animation: false,
			grid: { left: 52, right: 92, top: 28, bottom: 52 },
			legend: {
				bottom: 0,
				textStyle: { color: pal.axisText, fontSize: 11 },
				itemWidth: 14,
				itemHeight: 8,
				data: [
					'BT',
					...(v.hasEt ? ['ET'] : []),
					'ΔBT',
					...(v.hasEt ? ['ΔET'] : []),
					...(v.hasFan ? ['Fan'] : []),
					...(v.hasHeater ? ['Heater'] : [])
				]
			},
			tooltip: {
				trigger: 'axis',
				backgroundColor: pal.tooltipBg,
				borderColor: pal.line,
				textStyle: { color: pal.tooltipText, fontSize: 12 },
				axisPointer: { type: 'line', lineStyle: { color: pal.line } },
				formatter: (items: unknown) => {
					const list = items as {
						seriesName: string;
						value: [number, number | null];
						color: string;
					}[];
					if (!list.length) return '';
					const unit: Record<string, string> = {
						BT: tempUnit(),
						ET: tempUnit(),
						ΔBT: rorUnit(),
						ΔET: rorUnit(),
						Fan: '%',
						Heater: '%'
					};
					const rows = list
						.filter((i) => i.value?.[1] != null && unit[i.seriesName])
						.map(
							(i) =>
								`<span style="color:${i.color}">●</span> ${i.seriesName} <b>${i.value[1]}</b> ${unit[i.seriesName]}`
						);
					return [`<b>${fmtClock(list[0].value[0])}</b>`, ...rows].join('<br/>');
				}
			},
			xAxis: {
				type: 'value',
				min: 0,
				max: Math.ceil((end + 30) / 60) * 60,
				interval: 60,
				axisLabel: { ...axisLabel, hideOverlap: true, formatter: (s: number) => fmtClock(s) },
				axisLine: { lineStyle: { color: pal.line } },
				splitLine: { lineStyle: { color: pal.lineFaint } }
			},
			yAxis: [
				{
					type: 'value',
					name: tempUnit(),
					nameTextStyle: { color: pal.axisText },
					scale: true,
					axisLabel,
					splitLine: { lineStyle: { color: pal.line } }
				},
				{
					type: 'value',
					name: rorUnit(),
					nameTextStyle: { color: '#10b981' },
					position: 'right',
					min: 0,
					axisLabel: { ...axisLabel, color: '#10b981' },
					splitLine: { show: false }
				},
				{
					type: 'value',
					name: '%',
					nameTextStyle: { color: pal.axisText },
					position: 'right',
					offset: 44,
					min: 0,
					max: 100,
					axisLabel,
					splitLine: { show: false }
				}
			],
			series
		};
	});
</script>

<Chart {option} />
