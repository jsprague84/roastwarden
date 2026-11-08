<script lang="ts">
	import { resolve } from '$app/paths';
	import { chartPalette } from '#lib/constants/chartTheme.js';
	import { chartPrefs } from '#lib/stores/chartPrefs.svelte.js';
	import { smoothByTime, HEATER_SMOOTH_SEC } from '#lib/scope/smooth.js';
	import type { PageProps } from './$types';
	import { goto, refreshAll } from '$app/navigation';
	import Chart, { type ECOption } from '#lib/components/ui/Chart.svelte';
	import KeyTemperatures from '#lib/components/history/KeyTemperatures.svelte';
	import PhaseStatsPanel from '#lib/components/roast/PhaseStatsPanel.svelte';
	import CuppingEditor from '#lib/components/history/CuppingEditor.svelte';
	import SharePanel from '#lib/components/labels/SharePanel.svelte';
	import { landmarkColors, landmarkLabels } from '#lib/constants/landmarks.js';
	import type { SessionTelemetryPoint } from '#lib/api/sessions.js';
	import {
		PURPOSES,
		COLOR_SCALES,
		ROAST_FLAGS,
		parseFlags,
		joinFlags,
		flagLabel,
		purposeLabel,
		colorText,
		numOrNull
	} from '#lib/roast/properties.js';
	import { beans as beansApi, type Bean, type RoastCost } from '#lib/api/beans.js';
	import { downloadFile } from '#lib/api/http.js';
	import { profiles } from '#lib/api/profiles.js';
	import { roasted as roastedApi, type RoastedLot } from '#lib/api/roasted.js';
	import { sessions } from '#lib/api/sessions.js';
	import { lotSummary, stateClass } from '#lib/beans/roasted.js';
	import { fmtMoney, fmtPerKg } from '#lib/beans/money.js';
	import { batchLabel, fmtGrams } from '#lib/beans/stock.js';
	import { roastSummary, rorSeries } from '#lib/roast/report.js';
	import { channelLines } from '#lib/scope/scopeData.js';
	import { axisLayout, clockStep } from '#lib/scope/axes.js';
	import { channelColor } from '#lib/roaster/channels.js';
	import { labelRows, labelWidth } from '#lib/roast/labels.js';
	import {
		aucConfig,
		fcDropRorSlope,
		phaseAuc,
		DEFAULT_AUC,
		type AucConfig
	} from '#lib/roast/stats.js';
	import { settings as settingsApi } from '#lib/api/settings.js';
	import { notifications } from '#lib/stores/notifications.js';
	import {
		toDisplay,
		deltaToDisplay,
		fmtDelta,
		fmtTemp,
		tempUnit,
		rorUnit,
		weightUnit
	} from '#lib/stores/units.svelte.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';
	import GuidanceSummary from '#lib/components/history/GuidanceSummary.svelte';
	import BeanGuidePanel from '#lib/components/beans/BeanGuidePanel.svelte';
	import RangeBand from '#lib/components/ui/RangeBand.svelte';
	import { readRoast, mmss } from '#lib/guidance/summary.js';
	import { bandText, levelLabel, refBasis } from '#lib/guidance/references.js';
	import type { HistoryRoast } from '#lib/guidance/expect.js';
	import { isOn } from '#lib/stores/modules.svelte.js';

	const pal = $derived(chartPalette());

	let exporting = $state(false);

	let { data }: PageProps = $props();

	let sessionData = $derived(data.session);
	let sessionEvents = $derived(data.events);

	// --- Edit details ---
	let editName = $state('');
	let editOrigin = $state('');
	let editVariety = $state('');
	let editGreen = $state<number | null>(null);
	let editRoasted = $state<number | null>(null);
	let editNotes = $state('');
	let editPurpose = $state('');
	let editColorWhole = $state('');
	let editColorGround = $state('');
	let editColorScale = $state('agtron');
	let editMoistGreen = $state('');
	let editMoistRoasted = $state('');
	let editDensity = $state('');
	let editFlags = $state<string[]>([]);
	let editRating = $state(0);
	let editBean = $state('');
	let editBatch = $state<string | number | null>('');
	// Beans for the picker and the bean's name (archived too: an old roast keeps its bean).
	let beanList = $state<Bean[]>([]);
	$effect(() => {
		beansApi
			.list(true)
			.then((l) => (beanList = l))
			.catch(() => (beanList = []));
	});
	const bean = $derived(beanList.find((b) => b.id === sessionData.bean_id) ?? null);
	// Roasted coffee (docs/roasted.md): what is left of this roast, resting or ready.
	let lot = $state<RoastedLot | null>(null);
	$effect(() => {
		const { id, status, roasted_weight } = sessionData;
		if (status !== 'completed') {
			lot = null;
			return;
		}
		void roasted_weight;
		roastedApi
			.lot(id)
			.then((l) => (lot = l))
			.catch(() => (lot = null));
	});

	// Statistics (docs/statistics.md): AUC per phase and the RoR slope FC → DROP, from the
	// recorded curve with the AUC settings.
	let aucCfg = $state<AucConfig>(DEFAULT_AUC);
	$effect(() => {
		settingsApi
			.get()
			.then((v) => (aucCfg = aucConfig(v)))
			.catch(() => {});
	});
	const summary = $derived(roastSummary(sessionEvents, sessionData.telemetry ?? []));
	const auc = $derived(phaseAuc(summary, sessionData.telemetry ?? [], aucCfg));
	const rorSlope = $derived(fcDropRorSlope(summary, sessionData.telemetry ?? []));
	const aucTotal = $derived(auc.total ?? sessionData.auc_value ?? null);

	// Roast guidance (docs/roast-guidance.md): this roast against its profile's level and
	// against the profile's other roasts on this roaster.
	let history = $state<HistoryRoast[]>([]);
	$effect(() => {
		const { profile_id, device_id } = sessionData;
		if (!profile_id) {
			history = [];
			return;
		}
		profiles
			.history(profile_id, device_id)
			.then((h) => (history = h))
			.catch(() => (history = []));
	});
	const reading = $derived(
		readRoast({
			session: sessionData,
			events: sessionEvents,
			telemetry: sessionData.telemetry ?? [],
			history,
			fmt: {
				temp: (c) => `${fmtTemp(c)} ${tempUnit()}`,
				rate: (c) => `${fmtDelta(c, 0)} ${rorUnit()}`
			}
		})
	);
	const lvl = $derived(levelLabel(reading.exp.level));
	const n1 = (x: number) => String(Math.round(x * 10) / 10);
	const lossRange = (r: [number, number]) => `${n1(r[0])}–${n1(r[1])} %`;
	const timeRange = (r: [number, number]) => `${mmss(r[0])}–${mmss(r[1])}`;
	const tempRange = (r: [number, number]) => `${fmtTemp(r[0])}–${fmtTemp(r[1])} ${tempUnit()}`;
	const colourRange = (r: [number, number], src: string) =>
		src === 'default'
			? `ground Agtron ${bandText(r)}`
			: `ground Agtron ${Math.round(r[0])}–${Math.round(r[1])}`;
	const lossExtra = $derived(
		[
			reading.lossSuggests ?? '',
			refBasis('weight_loss_chaff'),
			refBasis('weight_loss_spread')
		].filter(Boolean)
	);
	const devExtra = $derived.by(() => {
		const d = reading.j.dtr_pct;
		const dtr = reading.facts.dtr_pct;
		if (dtr == null) return [refBasis('development_effect')];
		const range = d
			? ` (${lvl || 'expected'} ${n1(d.range[0])}–${n1(d.range[1])} %, judged loosely)`
			: '';
		return [
			`DTR ${Math.round(dtr)} %${range}: shown, but development is judged in seconds, since DTR targets come from drum roasters.`,
			refBasis('development_effect')
		];
	});
	let saving = $state(false);
	let editOpen = $state(false);

	$effect(() => {
		const s = sessionData;
		editName = s.name ?? '';
		editOrigin = s.bean_origin ?? '';
		editVariety = s.bean_variety ?? '';
		editGreen = s.green_weight ?? null;
		editRoasted = s.roasted_weight ?? null;
		editNotes = s.notes ?? '';
		const str = (v: number | null | undefined) => (v != null ? String(v) : '');
		editPurpose = s.roast_purpose ?? '';
		editColorWhole = str(s.color_whole);
		editColorGround = str(s.color_ground);
		editColorScale = s.color_scale ?? 'agtron';
		editMoistGreen = str(s.moisture_green);
		editMoistRoasted = str(s.moisture_roasted);
		editDensity = str(s.density_green);
		editFlags = parseFlags(s.roast_flags);
		editRating = s.rating ?? 0;
		editBean = s.bean_id ?? '';
		editBatch = str(s.batch_number);
	});

	async function saveDetails() {
		saving = true;
		try {
			await sessions.update(sessionData.id, {
				name: editName.trim() || undefined,
				bean_origin: editOrigin.trim(),
				bean_variety: editVariety.trim(),
				green_weight: editGreen ?? undefined,
				roasted_weight: editRoasted ?? undefined,
				notes: editNotes,
				roast_purpose: editPurpose || null,
				color_whole: numOrNull(editColorWhole),
				color_ground: numOrNull(editColorGround),
				color_scale:
					numOrNull(editColorWhole) != null || numOrNull(editColorGround) != null
						? editColorScale
						: null,
				moisture_green: numOrNull(editMoistGreen),
				moisture_roasted: numOrNull(editMoistRoasted),
				density_green: numOrNull(editDensity),
				roast_flags: joinFlags(editFlags),
				rating: editRating || null,
				bean_id: editBean || null,
				batch_number: numOrNull(editBatch)
			});
			notifications.add('Session details saved', 'success');
			editOpen = false;
			await refreshAll();
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			notifications.add(`Failed to save: ${msg}`, 'error');
		} finally {
			saving = false;
		}
	}

	async function handleDelete() {
		if (!confirm('Delete this session? This cannot be undone.')) return;
		try {
			await sessions.delete(sessionData.id);
			notifications.add('Session deleted', 'success');
			goto(resolve('/history'));
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			notifications.add(`Failed to delete session: ${msg}`, 'error');
		}
	}

	let savingProfile = $state(false);

	/** The server extracts points, events and curves (CHARGE to DROP), then the designer opens it. */
	async function handleSaveAsProfile() {
		if (!(sessionData.telemetry ?? []).length) {
			notifications.add('No telemetry data to create a profile from', 'warning');
			return;
		}
		savingProfile = true;
		try {
			const created = await profiles.fromSession(sessionData.id);
			notifications.add(
				`Saved as profile: ${created.points.length} points, ${created.events?.length ?? 0} events`,
				'success'
			);
			goto(resolve('/profiles/[id]/edit', { id: created.id }));
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			notifications.add(`Could not save as profile: ${msg}`, 'error');
		} finally {
			savingProfile = false;
		}
	}

	async function handleExport(format: 'csv' | 'artisan') {
		exporting = true;
		try {
			const dateStr = sessionData.start_time
				? new Date(sessionData.start_time).toISOString().slice(0, 10)
				: new Date(sessionData.created_at).toISOString().slice(0, 10);
			const safeName = sessionData.name.replace(/\s+/g, '_');
			const ext = format === 'csv' ? 'csv' : 'alog';
			const filename = `${safeName}_${dateStr}.${ext}`;
			await downloadFile(`/api/sessions/${sessionData.id}/export/${format}`, filename);
			notifications.add(`Exported as ${ext.toUpperCase()}`, 'success');
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err);
			notifications.add(`Export failed: ${msg}`, 'error');
		} finally {
			exporting = false;
		}
	}

	function formatDate(iso: string): string {
		return new Date(iso).toLocaleString();
	}

	function formatDuration(secs: number): string {
		const m = Math.floor(secs / 60);
		const s = secs % 60;
		return `${m}:${s.toString().padStart(2, '0')}`;
	}

	let chartWidth = $state(800);
	// Chart option from historical telemetry
	let chartOption = $derived.by<ECOption>(() => {
		const telemetry = sessionData?.telemetry ?? [];
		if (!telemetry.length)
			return {
				title: {
					text: 'No telemetry data',
					left: 'center',
					top: 'center',
					textStyle: { color: pal.mutedText }
				}
			};

		const times = telemetry.map((t: SessionTelemetryPoint) => t.elapsed_seconds * 1000);
		// Telemetry is stored in °C; plot in the display unit.
		const disp = (v: number | null | undefined) => (v == null ? null : toDisplay(v));
		const btData = telemetry.map((t: SessionTelemetryPoint, i: number) => [
			times[i],
			disp(t.bean_temp)
		]);
		const etData = telemetry.map((t: SessionTelemetryPoint, i: number) => [
			times[i],
			disp(t.env_temp)
		]);
		// Display only (Settings → This browser): averaged around each point, so no lag
		const heaterRaw = telemetry.map((t: SessionTelemetryPoint) => t.heater);
		const heaterShown =
			chartPrefs.heater === 'smoothed'
				? smoothByTime(times, heaterRaw, HEATER_SMOOTH_SEC * 1000, true)
				: heaterRaw; // times are ms
		const heaterData = telemetry.map((_t: SessionTelemetryPoint, i: number) => [
			times[i],
			heaterShown[i]
		]);
		const fanData = telemetry.map((t: SessionTelemetryPoint, i: number) => [times[i], t.fan]);
		// BT rate of rise (±15 s slope, as the report draws it), from 30 s after CHARGE to 15 s
		// before DROP: the smoothing window would otherwise reach into the charge and the fall.
		const chargeT = summary.chargeAt;
		const dropT = summary.drop ? summary.drop.t + chargeT : null;
		const rorData = rorSeries(telemetry, 0)
			.filter(([t]) => t >= chargeT + 30 && (dropT == null || t <= dropT - 15))
			.map(([t, v]) => [t * 1000, deltaToDisplay(v)]);
		const rorMax = Math.max(10, ...rorData.map((p) => p[1] as number));
		// Declared channels (docs/channels.md): °C on the temperature axis, % on the output
		// axis, another unit on a hidden axis of its own (0 to its top; the tooltip reads raw)
		const chLines = channelLines(
			telemetry.map((t: SessionTelemetryPoint) => t.extras ?? null),
			data.channels ?? [],
			toDisplay
		);
		const ownAxes = chLines.filter((l) => l.axis === 'own');
		const channelSeries = chLines.map((l, i) => ({
			name: l.axis === 'own' ? `${l.label} (${data.channels[i].unit})` : l.label,
			type: 'line' as const,
			yAxisIndex: l.axis === 'temp' ? 0 : l.axis === 'pct' ? 1 : 3 + ownAxes.indexOf(l),
			data: telemetry.map((_t: SessionTelemetryPoint, k: number) => [times[k], l.values[k]]),
			itemStyle: { color: channelColor(i) },
			lineStyle: { width: 1.5, type: 'dashed' as const },
			showSymbol: false
		}));
		const chTemps = chLines
			.filter((l) => l.axis === 'temp')
			.flatMap((l) => l.values.filter((v): v is number => v != null));

		const allTemps = [
			...telemetry
				.flatMap((t: SessionTelemetryPoint) =>
					[t.bean_temp, t.env_temp].filter((v): v is number => v != null)
				)
				.map(toDisplay),
			...chTemps
		];
		const maxTemp = Math.max(...allTemps);
		const minTemp = Math.min(...allTemps);
		const padding = (maxTemp - minTemp) * 0.1 || 20;

		// On a phone (as the Roast page's scope, lib/scope/axes.ts): narrower axes, units only
		// as their names, no % axis (heater and fan keep their 0–100 scale; hover reads them).
		const lay = axisLayout(chartWidth);
		const pctShown = lay.pct !== null;
		const gridLeft = lay.compact ? lay.temp + 4 : 50;
		const gridRight = lay.compact ? (rorData.length ? lay.ror + 6 : 12) : rorData.length ? 110 : 60;
		// Landmark labels stack in rows where they would overlap (a phone fits few side by side).
		const tMin = times[0];
		const tMax = times[times.length - 1];
		const plotPx = Math.max(1, chartWidth - gridLeft - gridRight);
		const pxPerMs = plotPx / Math.max(1, tMax - tMin);
		const markText = sessionEvents.map(
			(e: { event_type: string }) => landmarkLabels[e.event_type] ?? e.event_type
		);
		const rows = labelRows(
			sessionEvents.map((e: { elapsed_seconds: number }) => e.elapsed_seconds * 1000 - tMin),
			markText.map((t: string) => labelWidth(t)),
			pxPerMs
		);
		const rowCount = rows.length ? Math.max(...rows) + 1 : 1;
		const nameGap = 11 + rowCount * 12; // axis titles sit above the label rows
		const markLines = sessionEvents.map(
			(e: { event_type: string; elapsed_seconds: number }, i: number) => ({
				xAxis: e.elapsed_seconds * 1000,
				lineStyle: {
					color: landmarkColors[e.event_type] ?? pal.mutedText,
					type: 'dashed' as const,
					width: 1
				},
				label: {
					formatter: markText[i],
					fontSize: 10,
					color: pal.labelText,
					distance: 5 + rows[i] * 12
				}
			})
		);

		return {
			animation: false,
			grid: { left: gridLeft, right: gridRight, top: nameGap + 14, bottom: lay.compact ? 28 : 40 },
			tooltip: {
				trigger: 'axis',
				backgroundColor: pal.tooltipBg,
				borderColor: pal.line,
				textStyle: { color: pal.tooltipText }
			},
			xAxis: {
				type: 'value',
				name: lay.compact ? '' : 'Time',
				min: 'dataMin',
				max: 'dataMax',
				// 2:00 steps, not 1:40 (lib/scope/axes.ts)
				interval: clockStep((tMax - tMin) / 1000, plotPx / lay.xSpace) * 1000,
				nameTextStyle: { color: pal.axisText },
				axisLabel: {
					color: pal.axisText,
					fontSize: lay.font,
					hideOverlap: true,
					showMaxLabel: false,
					formatter: (val: number) => {
						const s = Math.floor(val / 1000);
						return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, '0')}`;
					}
				},
				axisLine: { lineStyle: { color: pal.line } },
				splitLine: { show: false }
			},
			yAxis: [
				{
					type: 'value',
					name: lay.compact ? tempUnit() : `Temp (${tempUnit()})`,
					nameGap,
					nameTextStyle: { color: pal.axisText, fontSize: lay.font },
					axisLabel: { color: pal.axisText, fontSize: lay.font },
					axisLine: { lineStyle: { color: pal.line } },
					min: Math.floor(minTemp - padding),
					max: Math.ceil(maxTemp + padding),
					splitLine: { lineStyle: { color: pal.lineFaint } }
				},
				{
					type: 'value',
					name: rorData.length ? '%' : 'Heater / fan %',
					position: 'right',
					nameGap,
					show: pctShown,
					nameTextStyle: { color: pal.axisText },
					axisLabel: { color: pal.axisText },
					axisLine: { lineStyle: { color: pal.line } },
					min: 0,
					max: 100,
					splitLine: { show: false }
				},
				{
					type: 'value',
					name: rorUnit(),
					position: 'right',
					offset: pctShown ? 50 : 0,
					show: rorData.length > 0,
					nameGap,
					nameTextStyle: { color: pal.axisText, fontSize: lay.font },
					axisLabel: { color: pal.axisText, fontSize: lay.font },
					axisLine: { show: true, lineStyle: { color: pal.line } },
					min: 0,
					max: Math.ceil(rorMax / 5) * 5,
					splitLine: { show: false }
				},
				...ownAxes.map((l) => ({ type: 'value' as const, show: false, min: 0, max: l.top ?? 1 }))
			],
			series: [
				{
					name: 'BT',
					type: 'line',
					data: btData,
					itemStyle: { color: '#f59e0b' },
					lineStyle: { width: 2 },
					showSymbol: false,
					markLine: markLines.length ? { data: markLines, silent: true } : undefined
				},
				{
					name: 'ET',
					type: 'line',
					data: etData,
					itemStyle: { color: '#60a5fa' },
					lineStyle: { width: 2 },
					showSymbol: false
				},
				{
					name: 'Heater',
					type: 'line',
					yAxisIndex: 1,
					data: heaterData,
					itemStyle: { color: '#f87171' },
					lineStyle: { width: 0 },
					areaStyle: { opacity: 0.2 },
					showSymbol: false
				},
				{
					name: 'Fan',
					type: 'line',
					yAxisIndex: 1,
					data: fanData,
					itemStyle: { color: '#a78bfa' },
					lineStyle: { width: 0 },
					areaStyle: { opacity: 0.15 },
					showSymbol: false
				},
				{
					name: 'RoR BT',
					type: 'line',
					yAxisIndex: 2,
					data: rorData,
					itemStyle: { color: '#10b981' },
					lineStyle: { width: 1.5 },
					showSymbol: false
				},
				...channelSeries
			]
		};
	});
</script>

<svelte:head>
	<title>{sessionData?.name ?? 'Roast'} | Roastwarden</title>
</svelte:head>

<div class="space-y-4">
	<div class="flex flex-wrap items-center gap-x-4 gap-y-2">
		<a href={resolve('/history')} class="text-sm text-muted-foreground hover:text-foreground"
			>&larr; History</a
		>
		<h1 class="text-2xl font-bold text-foreground">
			{#if sessionData.batch_number}<span class="mr-1 tabular-nums text-muted-foreground"
					>{batchLabel(sessionData.batch_number)}</span
				>{/if}{sessionData.name}
		</h1>
		<span
			class="inline-flex rounded-full px-2 py-0.5 text-xs font-medium
				{sessionData.status === 'completed'
				? 'bg-green-500/15 text-green-400'
				: sessionData.status === 'active'
					? 'bg-amber-500/15 text-amber-400'
					: 'bg-muted text-muted-foreground'}"
		>
			{sessionData.status === 'active' ? 'roasting' : sessionData.status}
		</span>
		<div class="ml-auto flex flex-wrap items-center gap-2">
			{#if isOn('reports')}
				<a
					href={resolve(`/reports?kind=roast&roast=${encodeURIComponent(sessionData.id)}`)}
					class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
					title="This roast's report: a PDF to keep or print (Reports)"
				>
					Report
				</a>
			{/if}
			{#if isOn('labels')}
				<a
					href={resolve('/history/[id]/labels', { id: sessionData.id })}
					class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
					title="Bag labels for this roast, on a sheet of sticker labels"
				>
					Labels…
				</a>
			{/if}
			{#if sessionData.status === 'completed'}
				<button
					onclick={() => handleExport('csv')}
					disabled={exporting}
					class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-50"
					title="Download CSV"
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						width="14"
						height="14"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
						><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline
							points="7 10 12 15 17 10"
						/><line x1="12" y1="15" x2="12" y2="3" /></svg
					>
					CSV
				</button>
				<button
					onclick={() => handleExport('artisan')}
					disabled={exporting}
					class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-50"
					title="Download Artisan (.alog)"
				>
					<svg
						xmlns="http://www.w3.org/2000/svg"
						width="14"
						height="14"
						viewBox="0 0 24 24"
						fill="none"
						stroke="currentColor"
						stroke-width="2"
						stroke-linecap="round"
						stroke-linejoin="round"
						><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline
							points="7 10 12 15 17 10"
						/><line x1="12" y1="15" x2="12" y2="3" /></svg
					>
					.alog
				</button>
			{/if}
			<button
				onclick={handleSaveAsProfile}
				disabled={savingProfile}
				class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-50"
				title="Save this roast as a profile: curve, heater/fan feedforward, events and the full background curve"
			>
				<svg
					xmlns="http://www.w3.org/2000/svg"
					width="14"
					height="14"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-linecap="round"
					stroke-linejoin="round"
					><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z" /><polyline
						points="17 21 17 13 7 13 7 21"
					/><polyline points="7 3 7 8 15 8" /></svg
				>
				{savingProfile ? 'Saving…' : 'Save as Profile'}
			</button>
			<a
				href={resolve(`/history/compare?ids=${sessionData.id}`)}
				class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
				title="Compare with other sessions"
			>
				Compare...
			</a>
			<button
				onclick={handleDelete}
				class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-red-400 hover:bg-red-500/15"
				title="Delete session"
			>
				<svg
					xmlns="http://www.w3.org/2000/svg"
					width="14"
					height="14"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-linecap="round"
					stroke-linejoin="round"
					><path d="M3 6h18" /><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" /><path
						d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"
					/><line x1="10" y1="11" x2="10" y2="17" /><line x1="14" y1="11" x2="14" y2="17" /></svg
				>
				Delete
			</button>
		</div>
	</div>

	<!-- Metadata -->
	<div class="flex flex-wrap items-center gap-4 text-sm text-muted-foreground">
		<span>Date: {formatDate(sessionData.created_at)}</span>
		{#if bean}<span
				>Bean: <a
					href={resolve('/beans/[id]', { id: bean.id })}
					class="text-foreground hover:underline"
					title="{fmtGrams(bean.stock_g)} in stock">{bean.name}</a
				></span
			>
		{:else if sessionData.bean_origin}<span
				>Bean: {sessionData.bean_origin}{sessionData.bean_variety
					? ` (${sessionData.bean_variety})`
					: ''}</span
			>{/if}
		{#if sessionData.green_weight}<span
				>Green: {fmtGrams(sessionData.green_weight, { precise: true })}</span
			>{/if}
		{#if sessionData.roasted_weight}<span
				>Roasted: {fmtGrams(sessionData.roasted_weight, { precise: true })}</span
			>{/if}
		{#if lot && lot.state !== 'unweighed'}<a
				href={resolve('/beans')}
				class="hover:underline {stateClass(lot.state)}"
				title="Roasted coffee on the Beans page"
				>{lotSummary(lot).replace('Roasted coffee: ', '')}</a
			>{/if}
		{#if sessionData.total_time_seconds}<span
				>Duration: <RangeBand
					j={reading.j.total_s}
					range={reading.j.total_s ? timeRange(reading.j.total_s.range) : ''}
					level={lvl}
					extra={[refBasis('total_s')]}>{formatDuration(sessionData.total_time_seconds)}</RangeBand
				></span
			>{/if}
		{#if sessionData.roast_purpose}<span class="rounded bg-muted px-1.5 py-0.5 text-foreground"
				>{purposeLabel(sessionData.roast_purpose)}</span
			>{/if}
		{#if colorText(sessionData.color_whole, sessionData.color_ground, sessionData.color_scale)}<span
				><RangeBand
					j={reading.j.color_ground}
					range={reading.j.color_ground
						? colourRange(reading.j.color_ground.range, reading.j.color_ground.source)
						: ''}
					level={lvl}
					extra={[refBasis('color_drift'), refBasis('color_whole_gap')]}
					>{colorText(
						sessionData.color_whole,
						sessionData.color_ground,
						sessionData.color_scale
					)}</RangeBand
				></span
			>{/if}
		{#if sessionData.rating}<span class="text-amber-400" title="Rating"
				>{'★'.repeat(sessionData.rating)}{'☆'.repeat(5 - sessionData.rating)}</span
			>{/if}
		{#each parseFlags(sessionData.roast_flags) as f (f)}<span
				class="rounded bg-red-500/10 px-1.5 py-0.5 text-red-400">{flagLabel(f)}</span
			>{/each}
		<button
			onclick={() => (editOpen = !editOpen)}
			class="rounded border border-border px-2 py-0.5 text-xs text-foreground hover:bg-accent"
		>
			{editOpen ? 'Close' : 'Edit details'}
		</button>
	</div>

	<GuidanceSummary lines={reading.lines} note={reading.note} />

	{#if editOpen}
		<form
			onsubmit={(e) => {
				e.preventDefault();
				saveDetails();
			}}
			class="grid gap-3 rounded-lg border border-border bg-card p-4 sm:grid-cols-2"
		>
			<label class="text-xs text-muted-foreground sm:col-span-2">
				Name
				<input
					bind:value={editName}
					class="mt-1 w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
				/>
			</label>
			<label class="text-xs text-muted-foreground">
				Bean from stock
				<select
					bind:value={editBean}
					class="mt-1 w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
				>
					<option value="">None (the green weight stays out of stock)</option>
					{#each beanList.filter((b) => !b.archived || b.id === editBean) as b (b.id)}
						<option value={b.id}>{b.name}</option>
					{/each}
				</select>
			</label>
			<label
				class="text-xs text-muted-foreground"
				title="Per roaster; the next roast continues from the highest"
			>
				Batch number
				<input
					type="number"
					step="1"
					min="1"
					bind:value={editBatch}
					class="mt-1 w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
				/>
			</label>
			<label class="text-xs text-muted-foreground">
				Bean origin
				<input
					bind:value={editOrigin}
					class="mt-1 w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
				/>
			</label>
			<label class="text-xs text-muted-foreground">
				Variety / process
				<input
					bind:value={editVariety}
					class="mt-1 w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
				/>
			</label>
			<label class="text-xs text-muted-foreground">
				Green weight ({weightUnit()})
				<WeightInput
					bind:grams={editGreen}
					class="mt-1 w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
				/>
			</label>
			<label class="text-xs text-muted-foreground">
				Roasted weight ({weightUnit()})
				<WeightInput
					bind:grams={editRoasted}
					class="mt-1 w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
				/>
			</label>
			<label class="text-xs text-muted-foreground">
				Roasted for
				<select
					bind:value={editPurpose}
					class="mt-1 w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
				>
					<option value="">—</option>
					{#each PURPOSES as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
				</select>
			</label>
			<div class="text-xs text-muted-foreground">
				Rating
				<div class="mt-1 flex gap-1 text-lg">
					{#each [1, 2, 3, 4, 5] as r (r)}
						<button
							type="button"
							onclick={() => (editRating = editRating === r ? 0 : r)}
							class="leading-none {r <= editRating ? 'text-amber-400' : 'text-muted-foreground/40'}"
							aria-label="{r} stars">★</button
						>
					{/each}
				</div>
			</div>
			<div class="grid grid-cols-3 gap-2 text-xs text-muted-foreground sm:col-span-2">
				<label>
					Colour scale
					<select
						bind:value={editColorScale}
						class="mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground"
					>
						{#each COLOR_SCALES as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
					</select>
				</label>
				<label>
					Whole bean
					<input
						type="number"
						step="0.1"
						bind:value={editColorWhole}
						class="mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground"
					/>
				</label>
				<label>
					Ground
					<input
						type="number"
						step="0.1"
						bind:value={editColorGround}
						class="mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground"
					/>
				</label>
			</div>
			<div class="grid grid-cols-3 gap-2 text-xs text-muted-foreground sm:col-span-2">
				<label>
					Moisture green (%)
					<input
						type="number"
						step="0.1"
						min="0"
						max="30"
						bind:value={editMoistGreen}
						class="mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground"
					/>
				</label>
				<label>
					Moisture roasted (%)
					<input
						type="number"
						step="0.1"
						min="0"
						max="30"
						bind:value={editMoistRoasted}
						class="mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground"
					/>
				</label>
				<label>
					Green density (g/L)
					<input
						type="number"
						step="1"
						min="0"
						bind:value={editDensity}
						class="mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground"
					/>
				</label>
			</div>
			<fieldset class="text-xs text-muted-foreground sm:col-span-2">
				<legend>Roast defects</legend>
				<div class="mt-1 flex flex-wrap gap-x-3 gap-y-1">
					{#each ROAST_FLAGS as f (f.value)}
						<label class="flex items-center gap-1 text-sm text-foreground">
							<input
								type="checkbox"
								checked={editFlags.includes(f.value)}
								onchange={(e) =>
									(editFlags = e.currentTarget.checked
										? [...editFlags, f.value]
										: editFlags.filter((x) => x !== f.value))}
								class="h-3.5 w-3.5"
							/>
							{f.label}
						</label>
					{/each}
				</div>
			</fieldset>
			<label class="text-xs text-muted-foreground sm:col-span-2">
				Notes
				<textarea
					bind:value={editNotes}
					rows="3"
					class="mt-1 w-full rounded border border-border bg-input px-3 py-1.5 text-sm text-foreground"
				></textarea>
			</label>
			<div class="flex justify-end gap-2 sm:col-span-2">
				<button
					type="button"
					onclick={() => (editOpen = false)}
					class="rounded-md border border-border px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent"
					>Cancel</button
				>
				<button
					type="submit"
					disabled={saving}
					class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>
					{saving ? 'Saving…' : 'Save'}
				</button>
			</div>
		</form>
	{/if}

	<!-- Chart -->
	<div class="rounded-lg border border-border bg-card p-4" style="height: 400px;">
		<div class="h-full" bind:clientWidth={chartWidth}><Chart option={chartOption} /></div>
	</div>

	<!-- Key Temperatures -->
	<KeyTemperatures
		telemetry={sessionData.telemetry ?? []}
		events={sessionEvents}
		fcJ={reading.j.fc_bt}
		dropJ={reading.j.drop_bt}
		range={tempRange}
		extra={[refBasis('temperatures')]}
	/>

	<!-- Roast Metrics -->
	{#if sessionData.weight_loss_pct != null || reading.facts.development_s != null || aucTotal != null || sessionData.max_ror != null || sessionData.profile_rmse != null || rorSlope != null || (data.cost && isOn('costs'))}
		<div class="grid gap-3 sm:grid-cols-3 lg:grid-cols-5">
			{#if sessionData.weight_loss_pct != null}
				<div class="rounded-lg border border-border bg-card p-3">
					<div class="text-xs font-medium text-muted-foreground">Weight Loss</div>
					<div class="mt-1 text-lg font-semibold text-foreground">
						<RangeBand
							j={reading.j.weight_loss_pct}
							range={reading.j.weight_loss_pct ? lossRange(reading.j.weight_loss_pct.range) : ''}
							level={lvl}
							extra={lossExtra}>{sessionData.weight_loss_pct.toFixed(1)}%</RangeBand
						>
					</div>
					{#if sessionData.green_weight != null && sessionData.roasted_weight != null}
						<div class="mt-0.5 text-xs text-muted-foreground">
							{fmtGrams(sessionData.green_weight, { precise: true })} → {fmtGrams(
								sessionData.roasted_weight,
								{ precise: true }
							)}
						</div>
					{/if}
				</div>
			{/if}
			{#if reading.facts.development_s != null}
				<div class="rounded-lg border border-border bg-card p-3">
					<div class="text-xs font-medium text-muted-foreground">Development</div>
					<div class="mt-1 text-lg font-semibold text-foreground">
						<RangeBand
							j={reading.j.development_s}
							range={reading.j.development_s ? timeRange(reading.j.development_s.range) : ''}
							level={lvl}
							extra={devExtra}>{mmss(reading.facts.development_s)}</RangeBand
						>
					</div>
					{#if reading.facts.dtr_pct != null}<div class="mt-0.5 text-xs text-muted-foreground">
							first crack → drop · DTR {Math.round(reading.facts.dtr_pct)} %
						</div>{/if}
				</div>
			{/if}
			{#if aucTotal != null}
				<div
					class="rounded-lg border border-border bg-card p-3"
					title="Area under BT from {aucCfg.start === 'charge'
						? 'CHARGE'
						: aucCfg.start.replace(/_/g, ' ')} to DROP{aucCfg.base
						? ' above the base temperature'
						: ''} (Settings → Roasting)"
				>
					<div class="text-xs font-medium text-muted-foreground">AUC</div>
					<div class="mt-1 text-lg font-semibold text-foreground">
						{fmtDelta(aucTotal, 0)}
						{tempUnit()}·min
					</div>
					{#if auc.drying != null || auc.maillard != null || auc.development != null}
						<div
							class="mt-0.5 text-xs tabular-nums text-muted-foreground"
							title="Drying · Maillard · development"
						>
							{fmtDelta(auc.drying, 0)} · {fmtDelta(auc.maillard, 0)} · {fmtDelta(
								auc.development,
								0
							)}
						</div>
					{/if}
				</div>
			{/if}
			{#if rorSlope != null}
				<div
					class="rounded-lg border border-border bg-card p-3"
					title="How fast RoR fell from first crack to drop: a least-squares line through RoR, per minute. Steadily falling is the usual aim; near 0 or rising means a flick."
				>
					<div class="text-xs font-medium text-muted-foreground">RoR slope FC → drop</div>
					<div class="mt-1 text-lg font-semibold text-foreground">
						{fmtDelta(rorSlope, 1, true)}
						{rorUnit()} per min
					</div>
				</div>
			{/if}
			{#if sessionData.max_ror != null}
				<div class="rounded-lg border border-border bg-card p-3">
					<div class="text-xs font-medium text-muted-foreground">Max RoR</div>
					<div class="mt-1 text-lg font-semibold text-foreground">
						{fmtDelta(sessionData.max_ror)}
						{rorUnit()}
					</div>
				</div>
			{/if}
			{#if data.cost && isOn('costs')}
				{@const c = data.cost as RoastCost}
				<div
					class="rounded-lg border border-border bg-card p-3"
					title="The green coffee at the bean's average cost when the roast started; per kg roasted is that ÷ the roasted weight. Doses and bag size: Settings → Roasting."
				>
					<div class="text-xs font-medium text-muted-foreground">Coffee cost</div>
					<div class="mt-1 text-lg font-semibold text-foreground">
						{fmtMoney(c.green_cost, c.currency)}
					</div>
					<div class="mt-0.5 text-xs tabular-nums text-muted-foreground">
						{#if c.roasted_per_kg != null}{fmtPerKg(c.roasted_per_kg, c.currency)} roasted · {fmtMoney(
								c.per_espresso_dose,
								c.currency
							)} per {c.espresso_dose_g} g espresso · {fmtMoney(c.per_filter_dose, c.currency)} per {c.filter_dose_g}
							g filter{:else}{fmtPerKg(c.green_per_kg, c.currency)} green; weigh the roasted beans for
							the rest{/if}
					</div>
				</div>
			{/if}
			{#if sessionData.profile_rmse != null}
				<div
					class="rounded-lg border border-border bg-card p-3"
					title="BT against the session's profile from the turning point to DROP: RMS, largest deviation, and the mean (negative: BT ran behind)"
				>
					<div class="text-xs font-medium text-muted-foreground">Profile tracking</div>
					<div class="mt-1 text-lg font-semibold text-foreground">
						±{fmtDelta(sessionData.profile_rmse, 1)}
						{tempUnit()} RMS
					</div>
					<div class="mt-0.5 text-xs text-muted-foreground">
						max {fmtDelta(sessionData.profile_max_dev ?? null, 1)} · mean {fmtDelta(
							sessionData.profile_bias ?? null,
							1,
							true
						)}
						{tempUnit()}
					</div>
				</div>
			{/if}
		</div>
	{/if}

	<!-- A new bean's next roast (docs/new-bean.md): shown while it has fewer than three on the profile -->
	{#if sessionData.bean_id && sessionData.status === 'completed'}
		{#key sessionData.id}
			<BeanGuidePanel
				beanId={sessionData.bean_id}
				after={sessionData.id}
				heading="h3"
				class="rounded-lg border border-border bg-card p-4"
			/>
		{/key}
	{/if}

	<!-- Phase Statistics -->
	<PhaseStatsPanel
		events={sessionEvents}
		totalTimeSeconds={sessionData.total_time_seconds ?? null}
		avgRorDrying={sessionData.avg_ror_drying}
		avgRorMaillard={sessionData.avg_ror_maillard}
		avgRorDevelopment={sessionData.avg_ror_development}
	/>

	<!-- Cupping Notes -->
	{#if isOn('cupping')}
		<details class="rounded-lg border border-border bg-card">
			<summary
				class="cursor-pointer px-4 py-3 text-sm font-semibold text-foreground hover:bg-accent"
			>
				Cupping Notes
			</summary>
			<div class="border-t border-border p-4">
				<CuppingEditor sessionId={sessionData.id} />
			</div>
		</details>
	{/if}

	<!-- The share page a bag's QR code opens (docs/labels.md, "The share page") -->
	{#if isOn('labels')}
		<details class="rounded-lg border border-border bg-card" id="share">
			<summary
				class="cursor-pointer px-4 py-3 text-sm font-semibold text-foreground hover:bg-accent"
			>
				Share page
			</summary>
			<div class="border-t border-border p-4">
				<SharePanel sessionId={sessionData.id} />
			</div>
		</details>
	{/if}
</div>
