<script lang="ts">
	/**
	 * Comparator (docs/analysis.md): up to 10 roasts aligned on a
	 * landmark, a reference roast, BT difference to it, smoothed RoR and a
	 * per-phase table. Statistics (docs/statistics.md): a consistency band
	 * (mean ± σ), AUC per phase, the RoR slope from FC to DROP, mean ± σ rows.
	 */
	import { resolve } from '$app/paths';
	import { clockStep } from '#lib/scope/axes.js';
	import { chartPalette } from '#lib/constants/chartTheme.js';
	import type { PageProps } from './$types';
	import Chart, { type ECOption } from '#lib/components/ui/Chart.svelte';
	import { events as eventsApi, type RoastEvent } from '#lib/api/events.js';
	import { sessions } from '#lib/api/sessions.js';
	import { settings as settingsApi } from '#lib/api/settings.js';
	import {
		aucConfig,
		consistencyBand,
		fcDropRorSlope,
		meanSd,
		phaseAuc,
		DEFAULT_AUC,
		type AucConfig,
		type PhaseAuc
	} from '#lib/roast/stats.js';
	import { batchLabel } from '#lib/beans/stock.js';
	import type { SessionWithTelemetry } from '#lib/api/sessions.js';
	import {
		toDisplay,
		deltaToDisplay,
		fmtTemp,
		tempUnit,
		rorUnit
	} from '#lib/stores/units.svelte.js';
	import {
		ALIGN_OPTIONS,
		alignOffset,
		alignedBt,
		deltaToReference,
		type AlignOn
	} from '#lib/roast/compare.js';
	import {
		roastSummary,
		phaseRows,
		rorSeries,
		fmtMmSs,
		type Summary,
		type PhaseRow
	} from '#lib/roast/report.js';
	import { purposeLabel, colorText } from '#lib/roast/properties.js';
	import { FileText } from '@lucide/svelte';
	import { isOn } from '#lib/stores/modules.svelte.js';

	const pal = $derived(chartPalette());
	let { data }: PageProps = $props();

	const COLORS = [
		'#f59e0b',
		'#60a5fa',
		'#34d399',
		'#f87171',
		'#a78bfa',
		'#f472b6',
		'#2dd4bf',
		'#facc15',
		'#94a3b8',
		'#fb923c'
	];
	const MAX = 10;

	interface Loaded {
		session: SessionWithTelemetry;
		events: RoastEvent[];
		color: string;
		sum: Summary;
		phases: PhaseRow[];
		auc: PhaseAuc;
		/** RoR slope FC → DROP, °C/min per min. */
		slope: number | null;
	}

	let auc = $state<AucConfig>(DEFAULT_AUC);
	$effect(() => {
		settingsApi
			.get()
			.then((s) => (auc = aucConfig(s)))
			.catch(() => {});
	});
	let showBand = $state(false);

	let loaded = $state<Loaded[]>([]);
	let loading = $state(true);
	let alignOn = $state<AlignOn>('charge');
	let refId = $state<string | null>(null);

	$effect(() => {
		const ids = data.ids.slice(0, MAX);
		if (ids.length === 0) {
			loading = false;
			return;
		}
		loading = true;
		Promise.all(
			ids.map(async (id, i) => {
				const [session, events] = await Promise.all([sessions.get(id), eventsApi.list(id)]);
				const tele = session.telemetry ?? [];
				const sum = roastSummary(events, tele);
				return {
					session,
					events,
					color: COLORS[i % COLORS.length],
					sum,
					phases: phaseRows(sum, tele),
					auc: phaseAuc(sum, tele),
					slope: fcDropRorSlope(sum, tele)
				} as Loaded;
			})
		)
			.then((r) => {
				loaded = r;
				refId = r[0]?.session.id ?? null;
			})
			.catch(() => (loaded = []))
			.finally(() => (loading = false));
	});

	// AUC follows the settings (base and start), loaded after the roasts.
	const aucs = $derived(
		new Map(loaded.map((l) => [l.session.id, phaseAuc(l.sum, l.session.telemetry ?? [], auc)]))
	);
	const aucOf = (l: Loaded) => aucs.get(l.session.id) ?? l.auc;

	const ref = $derived(loaded.find((l) => l.session.id === refId) ?? loaded[0] ?? null);
	// Two finished roasts make a compare report: the reference as A, the other as B
	const reportPair = $derived.by(() => {
		if (loaded.length !== 2 || !ref || loaded.some((l) => l.session.status !== 'completed'))
			return null;
		const other = loaded.find((l) => l !== ref);
		return other ? { a: ref.session.id, b: other.session.id } : null;
	});
	const aligned = $derived(
		loaded.map((l) => {
			const { offset, missing } = alignOffset(l.events, alignOn);
			const tele = l.session.telemetry ?? [];
			return { l, offset, missing, bt: alignedBt(tele, offset) };
		})
	);
	const missing = $derived(aligned.filter((a) => a.missing).map((a) => a.l.session.name));
	// Each roast's BT from its CHARGE to its DROP only: cooling is not part of the roast.
	const roastBt = $derived(
		aligned.map((a) => {
			const from = a.l.sum.chargeAt - a.offset;
			const to = from + (a.l.sum.total ?? Infinity);
			return a.bt.filter(([t]) => t >= from && t <= to);
		})
	);

	// Plot window: 30 s before the alignment point's earliest CHARGE to 60 s after the last DROP.
	const xRange = $derived.by(() => {
		let from = 0;
		let to = 60;
		for (const a of aligned) {
			const chargeAt = a.l.sum.chargeAt - a.offset;
			const end = chargeAt + (a.l.sum.total ?? 0);
			from = Math.min(from, chargeAt - 30);
			to = Math.max(to, end + 60);
		}
		return { from, to };
	});

	const mmss = (v: number) => fmtMmSs(v);

	function baseAxes(
		yName: string,
		y2?: string
	): Pick<ECOption, 'xAxis' | 'yAxis' | 'grid' | 'tooltip' | 'legend' | 'animation'> {
		const axis = {
			nameTextStyle: { color: pal.axisText },
			axisLabel: { color: pal.axisText },
			axisLine: { lineStyle: { color: pal.line } }
		};
		return {
			animation: false,
			grid: { left: 55, right: y2 ? 55 : 20, top: 36, bottom: 36 },
			legend: { show: true, top: 4, textStyle: { color: pal.axisText, fontSize: 10 } },
			tooltip: {
				trigger: 'axis',
				backgroundColor: pal.tooltipBg,
				borderColor: pal.line,
				textStyle: { color: pal.tooltipText }
			},
			xAxis: {
				type: 'value',
				min: xRange.from,
				max: xRange.to,
				name: `from ${ALIGN_OPTIONS.find((o) => o.value === alignOn)?.label}`,
				...axis,
				// 2:00 steps, not 1:40 (lib/scope/axes.ts)
				interval: clockStep(xRange.to - xRange.from, 10),
				axisLabel: { color: pal.axisText, formatter: mmss, hideOverlap: true },
				splitLine: { show: false }
			},
			yAxis: [
				{
					type: 'value',
					name: yName,
					scale: true,
					...axis,
					splitLine: { lineStyle: { color: pal.lineFaint } }
				},
				...(y2
					? [
							{
								type: 'value' as const,
								name: y2,
								position: 'right' as const,
								min: 0,
								...axis,
								splitLine: { show: false }
							}
						]
					: [])
			]
		};
	}

	const curves = $derived.by<ECOption>(() => {
		if (!loaded.length) return {};
		const series: ECOption['series'] = [];
		for (const a of aligned) {
			const isRef = a.l === ref;
			const name = a.l.session.name;
			series.push({
				name,
				type: 'line',
				data: a.bt.map(([t, v]) => [t, toDisplay(v)]),
				showSymbol: false,
				itemStyle: { color: a.l.color },
				lineStyle: { width: isRef ? 3 : 1.5, color: a.l.color }
			});
			const end = a.l.sum.total != null ? a.l.sum.chargeAt + a.l.sum.total - 15 : Infinity;
			const ror = rorSeries(a.l.session.telemetry ?? [], a.l.sum.chargeAt)
				.filter(([t]) => t >= 30 && a.l.sum.chargeAt + t <= end)
				.map(([t, v]) => [t + a.l.sum.chargeAt - a.offset, deltaToDisplay(v)]);
			series.push({
				name,
				type: 'line',
				yAxisIndex: 1,
				data: ror,
				showSymbol: false,
				itemStyle: { color: a.l.color },
				lineStyle: { width: 1, type: 'dashed', color: a.l.color }
			});
		}
		if (showBand && aligned.length >= 2) {
			// Mean ± σ of BT and RoR (docs/statistics.md): one polygon per band, the mean dotted.
			const ror = aligned.map((a) => {
				const end = a.l.sum.total != null ? a.l.sum.chargeAt + a.l.sum.total - 15 : Infinity;
				return rorSeries(a.l.session.telemetry ?? [], a.l.sum.chargeAt)
					.filter(([t]) => t >= 30 && a.l.sum.chargeAt + t <= end)
					.map(([t, v]) => [t + a.l.sum.chargeAt - a.offset, v] as [number, number]);
			});
			const bands: [string, [number, number][][], number, (v: number) => number][] = [
				['BT', roastBt, 0, toDisplay],
				['RoR', ror, 1, deltaToDisplay]
			];
			for (const [label, curvesIn, axis, conv] of bands) {
				const band = consistencyBand(curvesIn, xRange.from, xRange.to, 5);
				if (band.length < 2) continue;
				const upper = band.map((b) => [b.t, conv(b.mean + b.sd)]);
				const lower = band.map((b) => [b.t, conv(b.mean - b.sd)]).reverse();
				series.push(
					{
						name: `${label} ± σ`,
						type: 'custom',
						yAxisIndex: axis,
						silent: true,
						clip: true,
						z: 1,
						data: band.map((b) => [b.t, conv(b.mean)]),
						tooltip: { show: false },
						itemStyle: { color: pal.mutedText },
						renderItem: (
							params: { dataIndex: number },
							api: { coord: (p: number[]) => number[] }
						) =>
							params.dataIndex === 0
								? {
										type: 'polygon',
										shape: { points: [...upper, ...lower].map((p) => api.coord(p)) },
										style: { fill: pal.mutedText, opacity: 0.16 }
									}
								: null
					},
					{
						name: `${label} mean`,
						type: 'line',
						yAxisIndex: axis,
						data: band.map((b) => [b.t, conv(b.mean)]),
						showSymbol: false,
						lineStyle: { width: 1.5, type: 'dotted', color: pal.mutedText },
						itemStyle: { color: pal.mutedText }
					}
				);
			}
		}
		return { ...baseAxes(`BT (${tempUnit()})`, `RoR (${rorUnit()})`), series };
	});

	// σ of BT across the selection over the roast: one number for "how repeatable".
	const bandSpread = $derived.by(() => {
		if (aligned.length < 2) return null;
		const band = consistencyBand(roastBt, xRange.from, xRange.to, 5);
		if (!band.length) return null;
		return {
			mean: band.reduce((x, b) => x + b.sd, 0) / band.length,
			max: Math.max(...band.map((b) => b.sd))
		};
	});

	const stat = (f: (l: Loaded) => number | null | undefined) => meanSd(loaded.map(f));
	const aucText = (v: number | null | undefined) =>
		v == null ? '–' : deltaToDisplay(v).toFixed(0);

	const deltas = $derived.by<ECOption>(() => {
		const r = aligned.find((a) => a.l === ref);
		if (!r || aligned.length < 2) return {};
		const series: ECOption['series'] = [];
		for (const a of aligned) {
			if (a === r) continue;
			series.push({
				name: a.l.session.name,
				type: 'line',
				data: deltaToReference(r.bt, a.bt, xRange.from, xRange.to, 5).map(([t, d]) => [
					t,
					deltaToDisplay(d)
				]),
				showSymbol: false,
				itemStyle: { color: a.l.color },
				lineStyle: { width: 1.5, color: a.l.color },
				markLine: {
					silent: true,
					symbol: 'none',
					data: [{ yAxis: 0 }],
					lineStyle: { color: pal.mutedText, type: 'solid', width: 1 },
					label: { show: false }
				}
			});
		}
		return { ...baseAxes(`BT − ${ref?.session.name ?? 'reference'} (${tempUnit()})`), series };
	});

	const phase = (l: Loaded, k: PhaseRow['key']) => l.phases.find((p) => p.key === k);
	const pctText = (p: PhaseRow | undefined) =>
		p ? `${fmtMmSs(p.duration)} · ${p.pct?.toFixed(0) ?? '–'} %` : '–';
	const deltaText = (
		v: number | null | undefined,
		refV: number | null | undefined,
		unit: 'temp' | 's' | 'pct' | 'auc',
		isRef: boolean
	) => {
		if (isRef || v == null || refV == null) return '';
		const d = v - refV;
		if (Math.abs(d) < (unit === 's' ? 1 : 0.05)) return '';
		const sign = d > 0 ? '+' : '−';
		const a = Math.abs(d);
		return unit === 's'
			? ` (${sign}${fmtMmSs(a)})`
			: unit === 'temp' || unit === 'auc'
				? ` (${sign}${deltaToDisplay(a).toFixed(unit === 'auc' ? 0 : 1)})`
				: ` (${sign}${a.toFixed(1)})`;
	};
</script>

<svelte:head>
	<title>Compare roasts | Roastwarden</title>
</svelte:head>

<div class="space-y-4">
	<div class="flex items-center gap-4">
		<a href={resolve('/history')} class="text-sm text-muted-foreground hover:text-foreground"
			>&larr; History</a
		>
		<h1 class="text-2xl font-bold text-foreground">Compare roasts</h1>
		{#if reportPair && isOn('reports')}
			<a
				href={resolve(
					`/reports?kind=compare&a=${encodeURIComponent(reportPair.a)}&b=${encodeURIComponent(
						reportPair.b
					)}`
				)}
				class="ml-auto inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
				title="The reference as A and the other roast as B, on one page to print or keep (Reports → Compare two roasts)"
			>
				<FileText size={15} aria-hidden="true" />Compare report
			</a>
		{/if}
	</div>

	{#if loading}
		<div class="py-12 text-center text-muted-foreground">Loading roasts…</div>
	{:else if loaded.length === 0}
		<div class="py-12 text-center">
			<p class="text-muted-foreground">No roasts selected.</p>
			<a
				href={resolve('/history')}
				class="mt-2 inline-block text-sm text-foreground underline underline-offset-2"
				>Pick roasts in History</a
			>
		</div>
	{:else}
		<div class="flex flex-wrap items-center gap-3 text-sm">
			<label class="flex items-center gap-2 text-muted-foreground">
				Align on
				<select
					bind:value={alignOn}
					class="rounded-md border border-border bg-input px-2 py-1 text-foreground"
				>
					{#each ALIGN_OPTIONS as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
				</select>
			</label>
			<label class="flex items-center gap-2 text-muted-foreground">
				Reference
				<select
					bind:value={refId}
					class="rounded-md border border-border bg-input px-2 py-1 text-foreground"
				>
					{#each loaded as l (l.session.id)}<option value={l.session.id}>{l.session.name}</option
						>{/each}
				</select>
			</label>
			<label
				class="flex items-center gap-1.5 text-muted-foreground"
				title="Mean ± σ of BT and RoR across the roasts shown, where two or more have data"
			>
				<input
					type="checkbox"
					bind:checked={showBand}
					disabled={loaded.length < 2}
					class="h-4 w-4"
				/> Consistency band
			</label>
			{#if bandSpread}
				<span
					class="text-xs text-muted-foreground"
					title="Standard deviation of BT across these roasts, CHARGE to the last DROP"
				>
					BT spread σ {deltaToDisplay(bandSpread.mean).toFixed(1)}
					{tempUnit()} on average, {deltaToDisplay(bandSpread.max).toFixed(1)} at most
				</span>
			{/if}
			{#if missing.length}
				<span class="text-xs text-amber-400"
					>No {ALIGN_OPTIONS.find((o) => o.value === alignOn)?.label} in {missing.join(', ')}:
					aligned on CHARGE</span
				>
			{/if}
			{#if data.ids.length > MAX}<span class="text-xs text-muted-foreground"
					>Showing the first {MAX}.</span
				>{/if}
		</div>

		<div class="rounded-lg border border-border bg-card p-3" style="height: 430px;">
			<Chart option={curves} />
		</div>
		{#if loaded.length > 1}
			<div class="rounded-lg border border-border bg-card p-3" style="height: 330px;">
				<Chart option={deltas} />
			</div>
		{/if}

		<div class="overflow-x-auto rounded-lg border border-border bg-card">
			<table class="min-w-full text-sm">
				<thead class="text-xs text-muted-foreground">
					<tr class="border-b border-border">
						<th class="px-3 py-2"></th>
						<th class="px-3 py-2 text-left font-medium">Roast</th>
						<th class="px-3 py-2 text-left font-medium">For</th>
						<th class="px-3 py-2 text-right font-medium">Time</th>
						<th class="px-3 py-2 text-right font-medium">First crack</th>
						<th class="px-3 py-2 text-right font-medium">DTR %</th>
						<th class="px-3 py-2 text-right font-medium">Drying</th>
						<th class="px-3 py-2 text-right font-medium">Maillard</th>
						<th class="px-3 py-2 text-right font-medium">Development</th>
						<th class="px-3 py-2 text-right font-medium">Dev RoR ({rorUnit()})</th>
						<th
							class="px-3 py-2 text-right font-medium"
							title="How fast RoR falls from first crack to drop (a least-squares line), {rorUnit()} per minute"
							>RoR slope FC→drop</th
						>
						<th class="px-3 py-2 text-right font-medium">Drop ({tempUnit()})</th>
						<th
							class="px-3 py-2 text-right font-medium"
							title="Area under BT ({tempUnit()}·min), from {auc.start === 'charge'
								? 'CHARGE'
								: auc.start.replace(/_/g, ' ')} to DROP{auc.base
								? `, above ${fmtTemp(auc.base, 0)} ${tempUnit()}`
								: ''}">AUC</th
						>
						<th
							class="px-3 py-2 text-right font-medium"
							title="AUC per phase: drying · Maillard · development">AUC by phase</th
						>
						<th class="px-3 py-2 text-right font-medium">Loss %</th>
						<th class="px-3 py-2 text-left font-medium">Colour</th>
						<th class="px-3 py-2 text-right font-medium">RMS</th>
					</tr>
				</thead>
				<tbody>
					{#each loaded as l (l.session.id)}
						{@const isRef = l === ref}
						{@const s = l.session}
						<tr class="border-b border-border/50 {isRef ? 'bg-accent/40' : ''}">
							<td class="px-3 py-2"
								><span class="inline-block h-3 w-3 rounded-full" style="background-color: {l.color}"
								></span></td
							>
							<td class="max-w-[14rem] truncate px-3 py-2 text-foreground" title={s.name}>
								<a href={resolve('/history/[id]', { id: s.id })} class="hover:underline"
									>{#if s.batch_number}<span class="mr-1 tabular-nums text-muted-foreground"
											>{batchLabel(s.batch_number)}</span
										>{/if}{s.name}</a
								>{#if isRef}<span class="ml-1 text-xs text-muted-foreground">(ref)</span>{/if}
							</td>
							<td class="px-3 py-2 text-muted-foreground"
								>{s.roast_purpose ? purposeLabel(s.roast_purpose) : '–'}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{fmtMmSs(l.sum.total)}<span class="text-xs text-muted-foreground"
									>{deltaText(l.sum.total, ref?.sum.total, 's', isRef)}</span
								></td
							>
							<td class="px-3 py-2 text-right tabular-nums">
								{l.sum.fc
									? `${fmtMmSs(l.sum.fc.t)}${l.sum.fc.temp != null ? ` · ${fmtTemp(l.sum.fc.temp, 0)}` : ''}`
									: '–'}<span class="text-xs text-muted-foreground"
									>{deltaText(l.sum.fc?.t, ref?.sum.fc?.t, 's', isRef)}</span
								>
							</td>
							<td class="px-3 py-2 text-right tabular-nums"
								>{l.sum.dtr != null ? l.sum.dtr.toFixed(1) : '–'}<span
									class="text-xs text-muted-foreground"
									>{deltaText(l.sum.dtr, ref?.sum.dtr, 'pct', isRef)}</span
								></td
							>
							<td class="px-3 py-2 text-right tabular-nums text-muted-foreground"
								>{pctText(phase(l, 'drying'))}</td
							>
							<td class="px-3 py-2 text-right tabular-nums text-muted-foreground"
								>{pctText(phase(l, 'maillard'))}</td
							>
							<td class="px-3 py-2 text-right tabular-nums text-muted-foreground"
								>{pctText(phase(l, 'development'))}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{phase(l, 'development')?.ror != null
									? deltaToDisplay(phase(l, 'development')!.ror!).toFixed(1)
									: '–'}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{l.slope != null ? deltaToDisplay(l.slope).toFixed(1) : '–'}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{l.sum.drop?.temp != null ? fmtTemp(l.sum.drop.temp, 1) : '–'}<span
									class="text-xs text-muted-foreground"
									>{deltaText(l.sum.drop?.temp, ref?.sum.drop?.temp, 'temp', isRef)}</span
								></td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{aucText(aucOf(l).total)}<span class="text-xs text-muted-foreground"
									>{deltaText(aucOf(l).total, ref ? aucOf(ref).total : null, 'auc', isRef)}</span
								></td
							>
							<td class="whitespace-nowrap px-3 py-2 text-right tabular-nums text-muted-foreground"
								>{aucText(aucOf(l).drying)} · {aucText(aucOf(l).maillard)} · {aucText(
									aucOf(l).development
								)}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{s.weight_loss_pct != null ? s.weight_loss_pct.toFixed(1) : '–'}</td
							>
							<td class="px-3 py-2 text-muted-foreground"
								>{colorText(s.color_whole, s.color_ground, s.color_scale) ?? '–'}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{s.profile_rmse != null
									? `±${deltaToDisplay(s.profile_rmse).toFixed(1)}`
									: '–'}</td
							>
						</tr>
					{/each}
				</tbody>
				{#if loaded.length >= 2}
					{@const sd = (
						m: { mean: number; sd: number } | null,
						f: (v: number) => string,
						g: (v: number) => string = f
					) => (m ? `${f(m.mean)} ± ${g(m.sd)}` : '–')}
					<tfoot class="border-t border-border text-xs text-muted-foreground">
						<tr>
							<td class="px-3 py-2"></td>
							<td
								class="px-3 py-2 font-medium text-foreground"
								colspan="2"
								title="Mean ± sample standard deviation of the roasts above">Mean ± σ</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{sd(
									stat((l) => l.sum.total),
									fmtMmSs
								)}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{sd(
									stat((l) => l.sum.fc?.t),
									fmtMmSs
								)}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{sd(
									stat((l) => l.sum.dtr),
									(v) => v.toFixed(1)
								)}</td
							>
							<td class="px-3 py-2" colspan="4"></td>
							<td class="px-3 py-2 text-right tabular-nums"
								>{sd(
									stat((l) => l.slope),
									(v) => deltaToDisplay(v).toFixed(1)
								)}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{sd(
									stat((l) => l.sum.drop?.temp),
									(v) => fmtTemp(v, 1),
									(v) => deltaToDisplay(v).toFixed(1)
								)}</td
							>
							<td class="px-3 py-2 text-right tabular-nums"
								>{sd(
									stat((l) => aucOf(l).total),
									(v) => deltaToDisplay(v).toFixed(0)
								)}</td
							>
							<td class="px-3 py-2"></td>
							<td class="px-3 py-2 text-right tabular-nums"
								>{sd(
									stat((l) => l.session.weight_loss_pct),
									(v) => v.toFixed(1)
								)}</td
							>
							<td class="px-3 py-2" colspan="2"></td>
						</tr>
					</tfoot>
				{/if}
			</table>
		</div>
		<p class="text-xs text-muted-foreground">
			Differences in brackets are against the reference roast. Times are from CHARGE; DTR =
			development ÷ CHARGE→DROP.
		</p>
	{/if}
</div>
