<script lang="ts">
	/**
	 * The profile designer (docs/profiles.md): the profile's details, its points on a chart
	 * (click to add, drag to move) or in a table, the selected point, its events, and the
	 * transposer. The logic is lib/profile/designer.ts and designerChart.ts; the parts are
	 * the components under designer/.
	 */
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { ArrowRightLeft, LineChart, Table, Trash2 } from '@lucide/svelte';
	import { profiles, type ProfileCurves, type ProfileWithPoints } from '#lib/api/profiles.js';
	import EventsEditor from '#lib/components/profiles/designer/EventsEditor.svelte';
	import PointsTable from '#lib/components/profiles/designer/PointsTable.svelte';
	import ProfileFields from '#lib/components/profiles/designer/ProfileFields.svelte';
	import SelectedPoint from '#lib/components/profiles/designer/SelectedPoint.svelte';
	import ProfileTransposer from '#lib/components/profiles/ProfileTransposer.svelte';
	import Chart from '#lib/components/ui/Chart.svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import RangeBand from '#lib/components/ui/RangeBand.svelte';
	import { chartPalette } from '#lib/constants/chartTheme.js';
	import { PHASE_HINT } from '#lib/guidance/profile.js';
	import { levelLabel } from '#lib/guidance/references.js';
	import { mmss } from '#lib/guidance/summary.js';
	import {
		btCurve,
		designerEvents,
		designerFields,
		designerPoints,
		dtrInfo,
		newPoint,
		planReading,
		profileRequest,
		saveProblem,
		sortPoints
	} from '#lib/profile/designer.js';
	import { designerAxes, designerChartOption, handleGraphics } from '#lib/profile/designerChart.js';
	import { validateEvent } from '#lib/profile/events.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { fromDisplay, weightUnit } from '#lib/stores/units.svelte.js';

	const pal = $derived(chartPalette());

	// The designer works on an independent snapshot of the initial profile,
	// not a live reactive binding — intentionally capture-once.
	const { initialProfile = null }: { initialProfile?: ProfileWithPoints | null } = $props();

	// svelte-ignore state_referenced_locally
	const profileId = initialProfile?.id ?? null;
	// svelte-ignore state_referenced_locally
	let fields = $state(designerFields(initialProfile));
	let batchBad = $state(false);
	// svelte-ignore state_referenced_locally
	let points = $state(designerPoints(initialProfile));
	// svelte-ignore state_referenced_locally
	let events = $state(designerEvents(initialProfile));
	// Recorded background curves are read-only here; they can only be kept or removed.
	// svelte-ignore state_referenced_locally
	const curves: ProfileCurves | null = initialProfile?.curves ?? null;
	let removeCurves = $state(false);
	const eventErrors = $derived(events.map(validateEvent));
	let selectedIndex = $state<number | null>(null);
	let viewMode = $state<'chart' | 'table'>('chart');
	let saving = $state(false);
	let dragging = $state(false);
	let chartReady = $state(false);
	let showTransposer = $state(false);

	let chartComponent: ReturnType<typeof Chart> | undefined = $state();

	function addPoint(timeSec: number, temp: number) {
		const added = newPoint(timeSec, temp);
		points.push(added);
		points.sort((a, b) => a.time_seconds - b.time_seconds);
		selectedIndex = points.findIndex(
			(p) => p.time_seconds === added.time_seconds && p.target_temp === added.target_temp
		);
	}

	function deleteSelected() {
		if (selectedIndex === null || selectedIndex < 0 || selectedIndex >= points.length) return;
		points.splice(selectedIndex, 1);
		selectedIndex = null;
	}

	// Handle keyboard delete
	function handleKeydown(e: KeyboardEvent) {
		if ((e.target as HTMLElement | null)?.closest?.('input, select, textarea')) return;
		if (e.key === 'Delete' && selectedIndex !== null) {
			deleteSelected();
		}
	}

	// Chart click handler setup
	let chartHandlersAttached = false;

	$effect(() => {
		if (!chartComponent || chartHandlersAttached) return;
		const instance = chartComponent.getInstance();
		if (!instance) return;

		// Click on blank area (zrender level) to add a new point.
		// We must ignore clicks that land on a graphic element (existing point handle)
		// and also ignore clicks that are the "mouseup" of a drag gesture.
		instance
			.getZr()
			.on('click', (params: { target?: unknown; offsetX: number; offsetY: number }) => {
				if (params.target) return; // clicked on a graphic element or series symbol
				if (dragging) {
					dragging = false;
					return;
				}
				const dataPoint = instance.convertFromPixel('grid', [params.offsetX, params.offsetY]);
				if (dataPoint) {
					const [timeSec, tempDisp] = dataPoint as number[];
					const temp = fromDisplay(tempDisp);
					if (timeSec >= 0 && temp >= 0 && temp <= 350) {
						addPoint(timeSec, temp);
					}
				}
			});

		chartHandlersAttached = true;
		// Trigger a re-computation of chartOption now that the instance is ready
		// for convertToPixel/convertFromPixel calls
		chartReady = true;
	});

	// What the draggable handles do to the points (lib/profile/designerChart.ts draws them).
	const handles = {
		onDragBt(index: number, to: [number, number] | null) {
			dragging = true;
			if (!to) return;
			points[index].time_seconds = to[0];
			points[index].target_temp = to[1];
		},
		onDragEt(index: number, tempC: number | null) {
			dragging = true;
			if (tempC == null) return;
			points[index].target_env_temp = tempC;
		},
		onDragEnd(index: number) {
			points.sort((a, b) => a.time_seconds - b.time_seconds);
			if (index >= 0 && index < points.length) {
				const movedPt = points[index];
				selectedIndex = points.findIndex((p) => p === movedPt);
			}
		},
		onSelect(index: number | null) {
			selectedIndex = index;
		}
	};

	// Derived DTR preview: FC from the profile's FC landmark, else the BT threshold.
	const dtr = $derived(dtrInfo(sortPoints(points), events));

	// Roast guidance while editing (docs/roast-guidance.md): the planned development against
	// the level, the phase split (information only), and the results the level should give.
	const plan = $derived(planReading(dtr, sortPoints(points), events, fields));

	// Chart options — includes graphic overlay for draggable points. The handles are built on
	// the axes about to be applied, so they sit on the series after any change (a transpose).
	// Access chartReady to re-run this derivation after the chart instance is initialized.
	const chartOption = $derived.by(() => {
		const sorted = sortPoints(points);
		const axes = designerAxes(sorted);
		const instance = chartReady ? chartComponent?.getInstance() : null;
		const graphic = instance
			? handleGraphics(instance, points, selectedIndex, axes, pal, handles)
			: [];
		return designerChartOption({
			sorted,
			events,
			curves,
			showCurves: !removeCurves,
			axes,
			pal,
			graphic
		});
	});

	async function handleSave() {
		const problem = saveProblem({
			fields,
			points,
			batchBad,
			eventErrors,
			weightUnit: weightUnit()
		});
		if (problem) {
			notifications.add(problem, 'error');
			return;
		}
		saving = true;
		try {
			const req = profileRequest({ fields, points, events, initial: initialProfile, removeCurves });
			const saved = profileId ? await profiles.update(profileId, req) : await profiles.create(req);
			notifications.add(profileId ? 'Profile updated' : 'Profile created', 'success');
			goto(resolve('/profiles/[id]', { id: saved.id }));
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e);
			notifications.add(`Failed to save profile: ${msg}`, 'error');
		} finally {
			saving = false;
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="space-y-4">
	<div class="flex items-center justify-between">
		<h1 class="text-2xl font-bold text-foreground">
			{profileId ? 'Edit profile' : 'New profile'}
		</h1>
		<div class="flex gap-2">
			<button
				onclick={() =>
					goto(profileId ? resolve('/profiles/[id]', { id: profileId }) : resolve('/profiles'))}
				class="rounded-md border border-border px-4 py-2 text-sm font-medium text-foreground hover:bg-accent"
			>
				Cancel
			</button>
			<button
				onclick={handleSave}
				disabled={saving || !fields.name.trim()}
				class="rounded-md bg-go px-4 py-2 text-sm font-medium text-go-foreground hover:bg-go-hover disabled:opacity-50"
			>
				{saving ? 'Saving...' : 'Save'}
			</button>
		</div>
	</div>

	<ProfileFields bind:fields bind:batchBad />

	<!-- The points: a chart (click to add, drag to move) or a table -->
	<div class="rounded-lg border border-border bg-card p-4">
		<div class="mb-3 flex items-center justify-between">
			<h2 class="text-sm font-semibold text-foreground">
				Profile Points ({points.length})
			</h2>
			<div class="flex gap-1">
				<button
					onclick={() => (viewMode = 'chart')}
					class="rounded px-2 py-1 text-xs {viewMode === 'chart'
						? 'bg-amber-500/20 text-amber-400'
						: 'text-muted-foreground hover:text-foreground'}"
					title="Chart view"
				>
					<LineChart class="h-4 w-4" />
				</button>
				<button
					onclick={() => (viewMode = 'table')}
					class="rounded px-2 py-1 text-xs {viewMode === 'table'
						? 'bg-amber-500/20 text-amber-400'
						: 'text-muted-foreground hover:text-foreground'}"
					title="Table view"
				>
					<Table class="h-4 w-4" />
				</button>
				{#if points.length >= 2}
					<button
						onclick={() => (showTransposer = true)}
						class="rounded px-2 py-1 text-xs text-muted-foreground hover:text-foreground"
						title="Transpose profile"
					>
						<ArrowRightLeft class="h-4 w-4" />
					</button>
				{/if}
				{#if selectedIndex !== null}
					<button
						onclick={deleteSelected}
						class="ml-2 rounded px-2 py-1 text-xs text-red-400 hover:bg-red-500/10 hover:text-red-300"
						title="Delete selected point"
					>
						<Trash2 class="h-4 w-4" />
					</button>
				{/if}
			</div>
		</div>

		{#if viewMode === 'chart'}
			<p class="mb-2 text-xs text-muted-foreground">
				Click to add points. Drag points to move them. Click a point to select, then press Delete to
				remove.
			</p>
			<div style="height: 350px;">
				<Chart bind:this={chartComponent} option={chartOption} />
			</div>
			{#if dtr}
				<div
					class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 rounded border border-border/50 bg-card/50 px-3 py-1.5 text-xs text-muted-foreground"
				>
					<span
						>FC at {Math.floor(dtr.fcTime / 60)}:{(dtr.fcTime % 60)
							.toString()
							.padStart(2, '0')}</span
					>
					<span
						>Total: {Math.floor(dtr.totalTime / 60)}:{(dtr.totalTime % 60)
							.toString()
							.padStart(2, '0')}</span
					>
					{#if plan?.development_s != null}
						<span
							>Development <RangeBand
								j={plan.development}
								range={plan.development
									? `${mmss(plan.development.range[0])}–${mmss(plan.development.range[1])}`
									: ''}
								level={levelLabel(fields.roastLevel)}
								extra={[
									`DTR ${dtr.dtr} %${plan.dtr ? ` (${levelLabel(fields.roastLevel)} ${plan.dtr.range[0]}–${plan.dtr.range[1]} %, judged loosely)` : ''}.`
								]}
								><span class="font-medium text-foreground">{mmss(plan.development_s)}</span
								></RangeBand
							></span
						>
					{/if}
					<span>Est. DTR: <span class="text-foreground">{dtr.dtr}%</span></span>
					{#if plan?.phases}<span class="inline-flex items-center gap-1"
							>{plan.phases}
							<Hint
								inline
								help={{ line: 'Information only: phase shares are not judged.', more: PHASE_HINT }}
							/></span
						>{/if}
				</div>
				{#if plan?.expected}<p class="mt-1 text-xs text-muted-foreground">
						Expect · {plan.expected} Loss and colour are results: the roast page reads them against this.
					</p>{/if}
			{/if}
		{:else}
			<PointsTable
				bind:points
				bind:selectedIndex
				ondelete={(i) => {
					selectedIndex = i;
					deleteSelected();
				}}
			/>
		{/if}
	</div>

	{#if viewMode === 'chart'}
		<SelectedPoint bind:points index={selectedIndex} />
	{/if}

	<EventsEditor
		bind:events
		bind:removeCurves
		{curves}
		curve={btCurve(points)}
		errors={eventErrors}
	/>
</div>

{#if showTransposer}
	<ProfileTransposer
		points={sortPoints(points)}
		onApply={(transformed, timeScale) => {
			points = transformed;
			// Time-triggered events stretch with the curve.
			for (const e of events) {
				if (e.at_seconds != null) e.at_seconds = Math.round(e.at_seconds * timeScale);
			}
			selectedIndex = null;
			showTransposer = false;
		}}
		onCancel={() => (showTransposer = false)}
	/>
{/if}
