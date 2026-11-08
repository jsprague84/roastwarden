<script lang="ts">
	/**
	 * The roast, one step at a time: set up → charge → landmarks → drop →
	 * complete. Each stage leads with one primary button; everything else
	 * the stage allows sits below it. Keys 1-8 mark landmarks in roast order.
	 */
	import { beans as beansApi, type Bean, type BeanGuide } from '#lib/api/beans.js';
	import type { RoastEvent } from '#lib/api/events.js';
	import { plan as planApi, type PlannedRoast } from '#lib/api/plans.js';
	import { profiles, type RoastProfile } from '#lib/api/profiles.js';
	import { replay } from '#lib/api/replay.js';
	import { roastSequence, type SequenceConfig } from '#lib/api/sequence.js';
	import { resolve } from '$app/paths';
	import { firstRoastLine, guideText } from '#lib/beans/guide.js';
	import { batchLabel, levelClass, nextPlan, planLabel, stockNote } from '#lib/beans/stock.js';
	import { replayState, setReplayState } from '#lib/stores/telemetry.js';
	import { autoDropLine, cutBanner } from '#lib/replay/replay.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { chargeLine, dropLine } from '#lib/roast/sequence.js';
	import { allows } from '#lib/roaster/setup.js';
	import {
		roast,
		selectedDevice,
		startRoast,
		pauseRoast,
		resumeRoast,
		completeRoast,
		cancelRoast,
		markLandmark,
		unmarkLandmark,
		setPreviewProfile,
		controlLevel
	} from '#lib/stores/roast.svelte.js';
	import { profileState } from '#lib/stores/profile.svelte.js';
	import { roastStage, isVirtual, LANDMARK_ORDER } from '#lib/roast/live.js';
	import { primaryAction, LANDMARK_ACTION } from '#lib/roast/readout.js';
	import { HELP } from '#lib/constants/help.js';
	import { fmtDelta, fmtTemp, tempUnit, weightUnit } from '#lib/stores/units.svelte.js';
	import { fmtClock, profileDtr } from '#lib/scope/scopeData.js';
	import DeviceSelector from '#lib/components/app/DeviceSelector.svelte';
	import RoastAfterSummary from '#lib/components/history/RoastAfterSummary.svelte';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import {
		Play,
		Pause,
		Flag,
		Check,
		ChevronDown,
		FlaskConical,
		FileText,
		ArrowRight,
		X
	} from '@lucide/svelte';
	import { isOn } from '#lib/stores/modules.svelte.js';

	const s = $derived(roast.session);
	const stage = $derived(roastStage(s, roast.events));
	const action = $derived(primaryAction(s, roast.events));
	const virtual = $derived(isVirtual(roast.deviceId));
	const device = $derived(selectedDevice());
	const paused = $derived(s?.status === 'paused');
	const marked = (type: string): RoastEvent | undefined =>
		roast.events.find((e) => e.event_type === type);
	const chargeEv = $derived(marked('charge'));
	const dropEv = $derived(marked('drop'));
	const dropAt = $derived(
		chargeEv && dropEv ? dropEv.elapsed_seconds - chargeEv.elapsed_seconds : null
	);

	// ---- what CHARGE and DROP do to this roaster (docs/roast-sequence.md) ----
	let sequence = $state<SequenceConfig | null>(null);
	$effect(() => {
		const id = roast.deviceId;
		if (!id) {
			sequence = null;
			return;
		}
		roastSequence
			.get(id)
			.then((v) => (sequence = v.config))
			.catch(() => (sequence = null));
	});
	const hasProfile = $derived(!!(s?.profile_id ?? roast.previewProfileId));
	// With control off the sequence sends nothing; below full it does not start the drive
	// (docs/roaster-setup.md).
	const level = $derived(controlLevel());
	const chargeSays = $derived(
		allows(level, 'sequence') ? chargeLine(sequence, hasProfile && allows(level, 'drive')) : null
	);
	const dropSays = $derived(
		allows(level, 'sequence') ? dropLine(sequence, (c) => `${fmtTemp(c, 0)}${tempUnit()}`) : null
	);

	// ---- the roaster cut its heater mid-drive (docs/roast-sequence.md) ----
	// Seed the drive's state on load, so a cut made before a reload still shows.
	$effect(() => {
		const sid = s?.id;
		if (!sid) return;
		replay
			.get(sid)
			.then((st) => {
				if ((st.active || st.stopped_by_device || st.stopped_by_restart) && st.device_id)
					setReplayState(st as never);
			})
			.catch(() => {});
	});
	const cut = $derived(cutBanner($replayState, s?.id, !!dropEv));
	let cutDismissed = $state<string | null>(null);
	let resuming = $state(false);
	let cutNotified: string | null = null;
	$effect(() => {
		const run = $replayState?.run_id;
		if (cut && run && cutNotified !== run) {
			cutNotified = run;
			notifications.add(
				cut.restart
					? `Profile drive stopped at ${cut.at}: the server restarted. Resume picks up at profile ${cut.profileAt}.`
					: `Profile drive stopped at ${cut.at}: the roaster cut its heater (${cut.why}).`,
				'error',
				15000
			);
		}
	});
	async function resumeDrive() {
		if (!s || resuming) return;
		resuming = true;
		try {
			const st = await replay.resumeAfterCut(s.id);
			if (st && typeof st === 'object' && 'session_id' in st) setReplayState(st);
			notifications.add('Heater power on; Profile drive resumed', 'success');
		} catch (e) {
			notifications.add(`Could not resume: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			resuming = false;
		}
	}

	// ---- auto DROP (docs/replay.md): the drive marks DROP itself at the profile's drop ----
	const autoDrop = $derived(autoDropLine($replayState, s?.id, !!dropEv));
	let autoDropBusy = $state(false);
	async function setAutoDrop(on: boolean) {
		if (!s || autoDropBusy) return;
		autoDropBusy = true;
		try {
			setReplayState(await replay.update(s.id, { auto_drop: on }));
			notifications.add(
				on ? 'Auto DROP is back on' : 'Auto DROP cancelled: drop by hand',
				on ? 'success' : 'warning'
			);
		} catch (e) {
			notifications.add(`Auto DROP: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			autoDropBusy = false;
		}
	}

	// Cancel asks on the page (the viewer of a phone has no dialogs to spare).
	let confirmCancel = $state(false);
	$effect(() => {
		void s?.id;
		confirmCancel = false;
	});

	// ---- setup ----
	let profileList = $state<RoastProfile[]>([]);
	$effect(() => {
		profiles
			.list()
			.then((l) => (profileList = l))
			.catch(() => (profileList = []));
	});
	let showDetails = $state(false);
	let name = $state('');
	let origin = $state('');
	let variety = $state('');
	// Grams (the weight field reads the browser's unit)
	let weight = $state<number | null>(null);
	let weightBad = $state(false);
	let notes = $state('');
	let beanId = $state('');

	const preview = $derived(profileState.activeProfile && !s ? profileState.activeProfile : null);

	// ---- planned roasts and beans (docs/batches.md) ----
	let planList = $state<PlannedRoast[]>([]);
	let beanList = $state<Bean[]>([]);
	let skipped = $state<Set<string>>(new Set());
	let unplanned = $state(false);
	$effect(() => {
		const dev = roast.deviceId;
		void s?.id;
		if (!dev || s) return;
		planApi
			.list(dev)
			.then((l) => (planList = l))
			.catch(() => (planList = []));
		beansApi
			.list()
			.then((l) => (beanList = l))
			.catch(() => (beanList = []));
	});
	const next = $derived(stage === 'setup' && !unplanned ? nextPlan(planList, skipped) : null);
	const planned = $derived(next?.plan ?? null);
	// Load the plan into the form once, when it becomes the one offered.
	let appliedPlan: string | null = null;
	$effect(() => {
		const p = planned;
		if (!p || p.id === appliedPlan) return;
		appliedPlan = p.id;
		setPreviewProfile(p.profile_id ?? '');
		beanId = p.bean_id ?? '';
		weight = p.green_weight_g ?? null;
		name = p.name ?? '';
	});
	function leavePlan(skip: boolean) {
		if (planned && skip) skipped = new Set([...skipped, planned.id]);
		else unplanned = true;
		appliedPlan = null;
		beanId = '';
		weight = null;
		name = '';
	}
	const bean = $derived(beanList.find((b) => b.id === beanId) ?? null);
	const weightG = $derived(weight);
	const beanNote = $derived(bean ? stockNote(bean, weightG) : null);

	// ---- a bean's first roast on this roaster (docs/new-bean.md): one line, the rest in its "?" ----
	let guide = $state<BeanGuide | null>(null);
	$effect(() => {
		const b = beanId;
		const dev = roast.deviceId;
		const prof = roast.previewProfileId;
		if (stage !== 'setup' || !b) {
			guide = null;
			return;
		}
		beansApi
			.guide(b, { profile: prof || null, roaster: dev })
			.then((g) => (guide = g.bean_id === beanId ? g : guide))
			.catch(() => (guide = null));
	});
	const firstLine = $derived(
		firstRoastLine(guide, {
			temp: (c) => fmtTemp(c, 0),
			delta: (c) => fmtDelta(c, 0),
			unit: tempUnit()
		})
	);

	async function start() {
		if (weightBad) {
			notifications.add(
				`The green weight is not a weight: type a number of ${weightUnit()}, or 100 g, 3.5 oz`,
				'error'
			);
			return;
		}
		await startRoast({
			// A roast from a bean is named after it (History has the date).
			name: name || bean?.name || undefined,
			bean_id: beanId || undefined,
			planned_roast_id: planned?.id,
			bean_origin: bean ? undefined : origin || undefined,
			bean_variety: bean ? undefined : variety || undefined,
			green_weight:
				weightG != null && Number.isFinite(weightG) && weightG > 0 ? weightG : undefined,
			notes: notes || undefined
		});
		if (roast.session) {
			name = origin = variety = notes = beanId = '';
			weight = null;
			showDetails = false;
			unplanned = false;
			skipped = new Set();
			appliedPlan = null;
		}
	}

	function doPrimary() {
		if (roast.busy) return;
		if (action.kind === 'start') start();
		else if (action.kind === 'resume') resumeRoast();
		else if (action.kind === 'mark') markLandmark(action.landmark);
		else completeRoast();
	}

	async function unmark(ev: RoastEvent) {
		if (!confirm(`Remove the ${LANDMARK_ACTION[ev.event_type] ?? ev.event_type} mark?`)) return;
		await unmarkLandmark(ev);
	}

	function complete() {
		if (
			stage !== 'dropped' &&
			!confirm('Complete the roast without a DROP mark? It is saved as it is.')
		)
			return;
		completeRoast();
	}

	// Keys 1-8 mark landmarks in roast order (Artisan style), not while typing.
	$effect(() => {
		const onKey = (e: KeyboardEvent) => {
			if (!s || s.status !== 'active') return;
			if (e.ctrlKey || e.metaKey || e.altKey) return;
			const el = e.target as HTMLElement | null;
			if (
				el &&
				(el.tagName === 'INPUT' ||
					el.tagName === 'TEXTAREA' ||
					el.tagName === 'SELECT' ||
					el.isContentEditable)
			)
				return;
			const idx = Number(e.key) - 1;
			const type = LANDMARK_ORDER[idx];
			if (!type || marked(type)) return;
			e.preventDefault();
			markLandmark(type);
		};
		window.addEventListener('keydown', onKey);
		return () => window.removeEventListener('keydown', onKey);
	});

	const STEPS = [
		{ id: 'setup', label: 'Set up' },
		{ id: 'charge', label: 'Charge' },
		{ id: 'roasting', label: 'Roast' },
		{ id: 'dropped', label: 'Finish' }
	] as const;
	const stepIndex = $derived(STEPS.findIndex((x) => x.id === stage));

	const primaryLabel = $derived(
		action.kind === 'start'
			? planned
				? 'Start planned roast'
				: 'Start roast'
			: action.kind === 'resume'
				? 'Resume recording'
				: action.kind === 'mark'
					? LANDMARK_ACTION[action.landmark]
					: 'Complete roast'
	);
	const primaryKey = $derived(
		action.kind === 'mark' ? String(LANDMARK_ORDER.indexOf(action.landmark as never) + 1) : null
	);
	const primaryClass = $derived(
		action.kind === 'mark' && action.landmark === 'drop'
			? 'bg-violet-600 hover:bg-violet-700 text-white'
			: action.kind === 'mark' && action.landmark !== 'charge'
				? 'bg-action hover:bg-action-hover text-action-foreground'
				: 'bg-go hover:bg-go-hover text-go-foreground'
	);

	function profileFacts(p: {
		target_first_crack?: number | null;
		target_total_time: number | null;
		target_end_temp: number | null;
		charge_temp: number | null;
	}) {
		const out: string[] = [];
		if (p.target_first_crack) out.push(`FC ${fmtClock(p.target_first_crack)}`);
		if (p.target_total_time)
			out.push(
				`drop ${fmtClock(p.target_total_time)}${p.target_end_temp ? ` @ ${fmtTemp(p.target_end_temp, 0)}${tempUnit()}` : ''}`
			);
		const dtr = profileDtr(p.target_first_crack, p.target_total_time);
		if (dtr) out.push(`DTR ${dtr.toFixed(0)} %`);
		return out.join(' · ');
	}

	const field =
		'w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const secondary =
		'inline-flex items-center justify-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs font-medium text-foreground hover:bg-accent disabled:opacity-40';
</script>

<section class="rounded-lg border border-border bg-card p-3" aria-label="Next step">
	<!-- Progress, and which roaster this is -->
	<div class="mb-3 flex items-center gap-2 text-xs">
		<ol class="flex items-center gap-1.5 font-medium" aria-label="Roast stage">
			{#each STEPS as step, i (step.id)}
				{#if i > 0}<li aria-hidden="true" class="text-muted-foreground/50">›</li>{/if}
				<li
					aria-current={i === stepIndex ? 'step' : undefined}
					class={i === stepIndex
						? 'text-highlight'
						: i < stepIndex
							? 'text-foreground'
							: 'text-muted-foreground'}
				>
					{step.label}
				</li>
			{/each}
		</ol>
		<span
			class="ml-auto hidden min-w-0 truncate text-muted-foreground md:inline"
			title="Change the roaster in the header"
		>
			{device?.name ?? roast.deviceId ?? 'no roaster'}
		</span>
		{#if virtual}
			<span
				class="inline-flex shrink-0 items-center gap-1 text-[10px] font-semibold uppercase text-emerald-400 max-md:ml-auto"
			>
				<FlaskConical class="h-3 w-3" /> Sim
			</span>
		{:else if device?.status === 'pending'}
			<span class="shrink-0 text-[10px] font-semibold uppercase text-amber-400 max-md:ml-auto"
				>Not activated</span
			>
		{/if}
	</div>

	{#if stage === 'setup'}
		{#if roast.lastCompleted}
			<div
				class="mb-3 flex flex-wrap items-center gap-2 rounded-md border border-green-500/30 bg-green-500/10 px-3 py-2 text-sm"
			>
				<Check class="h-4 w-4 text-green-400" />
				<span class="min-w-0 flex-1 truncate text-foreground"
					>Saved: {roast.lastCompleted.name}</span
				>
				<a
					href={resolve('/history/[id]', { id: roast.lastCompleted.id })}
					class="inline-flex items-center gap-1 font-medium text-green-400 hover:underline"
				>
					Review <ArrowRight class="h-3.5 w-3.5" />
				</a>
				{#if isOn('reports')}
					<a
						href={resolve('/history/[id]/report', { id: roast.lastCompleted.id })}
						class="inline-flex items-center gap-1 text-muted-foreground hover:text-foreground"
						title="Printable report"
					>
						<FileText class="h-3.5 w-3.5" />
					</a>
				{/if}
			</div>
			<RoastAfterSummary session={roast.lastCompleted} onsaved={(u) => (roast.lastCompleted = u)} />
		{/if}

		<div class="space-y-3">
			{#if planned && next}
				<div class="text-xs">
					<div class="flex items-baseline gap-2">
						<span class="shrink-0 font-semibold text-highlight"
							>Planned {next.index + 1} of {next.count}</span
						>
						<span class="min-w-0 flex-1 truncate text-foreground" title={planLabel(planned)}
							>{planLabel(planned, { profile: false })}</span
						>
						<button
							class="shrink-0 text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
							onclick={() => leavePlan(true)}
							title="Offer the next planned roast">Skip</button
						>
						<button
							class="shrink-0 text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
							onclick={() => leavePlan(false)}
							title="Roast something not in the plan">Unplanned</button
						>
					</div>
					{#if beanNote && beanNote.level !== 'ok'}<p class="mt-0.5 {levelClass(beanNote.level)}">
							{bean?.name}: {beanNote.text}
						</p>{/if}
				</div>
			{:else if unplanned && planList.some((p) => !p.session_id)}
				<button
					class="text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
					onclick={() => ((unplanned = false), (skipped = new Set()))}
				>
					Back to the plan ({planList.filter((p) => !p.session_id).length})
				</button>
			{/if}
			{#if firstLine && guide}
				<p class="flex items-center gap-1.5 text-xs text-muted-foreground">
					<span class="min-w-0 truncate" title={firstLine}>{firstLine}</span>
					<Hint inline help={{ line: firstLine, more: guideText(guide) }} />
				</p>
			{/if}
			<!-- The header has the picker from tablet width up. -->
			<div class="md:hidden">
				<span class="mb-1 block text-xs font-medium text-muted-foreground">Roaster</span>
				<DeviceSelector compact />
			</div>

			<div>
				<label for="next-profile" class="mb-1 block text-xs font-medium text-muted-foreground"
					>Profile</label
				>
				<select
					id="next-profile"
					class={field}
					value={roast.previewProfileId ?? ''}
					onchange={(e) => setPreviewProfile(e.currentTarget.value)}
				>
					<option value="">No profile: record only</option>
					{#each profileList as p (p.id)}
						<option value={p.id}>{p.name}</option>
					{/each}
				</select>
				{#if preview}
					<p class="mt-1 text-xs text-muted-foreground">
						{profileFacts(preview) || 'Shown on the scope from "now".'}
					</p>
				{:else}
					<p class="mt-1 text-xs text-muted-foreground">
						Drawn on the scope; it can also drive the roaster (Profile drive).
					</p>
				{/if}
			</div>

			<div>
				<button
					type="button"
					class="flex w-full items-center gap-1 text-xs font-medium text-muted-foreground hover:text-foreground"
					aria-expanded={showDetails}
					onclick={() => (showDetails = !showDetails)}
				>
					<ChevronDown class="h-3.5 w-3.5 transition-transform {showDetails ? 'rotate-180' : ''}" />
					Batch details (optional)
				</button>
				{#if showDetails}
					<div class="mt-2 grid grid-cols-2 gap-2">
						<input bind:value={name} placeholder="Roast name" class="{field} col-span-2" />
						<select bind:value={beanId} class="{field} col-span-2" aria-label="Bean from stock">
							<option value=""
								>{beanList.length ? 'No bean from stock' : 'No beans in stock (Beans page)'}</option
							>
							{#each beanList as b (b.id)}
								<option value={b.id}>{b.name}</option>
							{/each}
						</select>
						{#if !bean}
							<input bind:value={origin} placeholder="Origin" class={field} />
							<input bind:value={variety} placeholder="Variety" class={field} />
						{/if}
						<WeightInput
							bind:grams={weight}
							bind:invalid={weightBad}
							placeholder="Green weight ({weightUnit()})"
							class={field}
						/>
						<input bind:value={notes} placeholder="Notes" class={field} />
					</div>
					{#if beanNote}<p class="mt-1 text-[11px] {levelClass(beanNote.level)}">
							{beanNote.text}
						</p>{/if}
					<p class="mt-1 text-[11px] text-muted-foreground">
						Roast level, colour, moisture and cupping go on the roast's page afterwards.
					</p>
				{/if}
			</div>
		</div>
	{:else}
		<!-- Where the roast is (the time is in the readouts) -->
		<div class="flex items-baseline justify-between gap-3 text-sm">
			<span class="shrink-0 font-semibold {paused ? 'text-amber-400' : 'text-foreground'}">
				{#if paused}Paused{:else if stage === 'charge'}Recording, not charged{:else if stage === 'dropped'}Dropped
					at {dropAt != null ? fmtClock(dropAt) : '—'}{:else}Roasting{/if}
			</span>
			<span
				class="min-w-0 truncate text-xs text-muted-foreground"
				title="{s?.name ?? ''}{profileState.activeProfile
					? ` · ${profileState.activeProfile.name}`
					: ''}"
			>
				{#if s?.batch_number}<span class="mr-1 tabular-nums">{batchLabel(s.batch_number)}</span
					>{/if}{s?.name}{#if profileState.activeProfile}{' · '}{profileState.activeProfile
						.name}{/if}
			</span>
		</div>
		{#if paused}
			<p class="mt-2 border-l-2 border-amber-500 pl-2 text-xs text-amber-400">
				Recording is paused: telemetry is not saved and landmarks cannot be marked. The roaster
				keeps doing what it was doing.
			</p>
		{/if}
	{/if}

	{#if cut && cutDismissed !== $replayState?.run_id}
		<!-- Below 820 px tall the explanation is the tooltip only: the rail must not scroll -->
		{@const said = cut.restart
			? `Resume picks up at profile ${cut.profileAt}, where it stopped, with the old settings.`
			: `${cut.why}. It is in Manual with heater power off; the fan is at the roaster's safe setting.`}
		<div
			class="mt-3 border-l-2 border-red-500 pl-2 text-xs"
			role="alert"
			title={cut.restart
				? `${said} The roaster went to its safe state when its control lease ran out, or held its last output (no lease). The rest of the profile moves later by the time lost.`
				: said}
		>
			<p class="font-semibold text-red-400">
				Profile drive stopped at {cut.at}: {cut.restart
					? 'the server restarted'
					: 'the roaster cut its heater'}.
			</p>
			<p class="mt-0.5 text-muted-foreground [@media(max-height:820px)]:hidden">{said}</p>
			<div class="mt-1.5 flex flex-wrap gap-2 [@media(max-height:820px)]:mt-1">
				<button
					class="rounded-md bg-go px-2.5 py-1 font-medium text-go-foreground hover:bg-go-hover disabled:opacity-50"
					onclick={resumeDrive}
					disabled={resuming || roast.busy}
					title={cut.restart
						? `Heater power on from the start output, then Profile drive picks up at profile ${cut.profileAt}`
						: 'Heater power on from the start output, then Profile drive picks up at the current profile time'}
				>
					{resuming ? 'Resuming…' : 'Heater on and resume drive'}
				</button>
				<button class={secondary} onclick={() => (cutDismissed = $replayState?.run_id ?? null)}
					>Roast by hand</button
				>
			</div>
		</div>
	{/if}

	<!-- Primary action -->
	<div class="mt-3 flex gap-2">
		<button
			type="button"
			onclick={doPrimary}
			disabled={roast.busy || !roast.deviceId}
			class="flex h-12 flex-1 items-center justify-center gap-2 rounded-md px-4 text-base font-semibold transition-colors disabled:opacity-50 {primaryClass}"
		>
			{#if action.kind === 'start' || action.kind === 'resume'}<Play
					class="h-5 w-5"
				/>{:else if action.kind === 'complete'}<Check class="h-5 w-5" />{:else}<Flag
					class="h-5 w-5"
				/>{/if}
			{roast.busy ? '…' : primaryLabel}
			{#if primaryKey}<kbd class="ml-1 rounded border border-white/40 px-1.5 text-xs font-normal"
					>{primaryKey}</kbd
				>{/if}
		</button>
		{#if stage === 'roasting' && !paused && action.kind === 'mark' && action.landmark !== 'drop'}
			<button
				type="button"
				onclick={() => markLandmark('drop')}
				disabled={roast.busy}
				class="flex h-12 items-center justify-center gap-1.5 rounded-md bg-violet-600 px-4 text-base font-semibold text-white hover:bg-violet-700 disabled:opacity-50"
				title={`Drop (key 7). ${dropSays ?? (virtual ? HELP.dropVirtual.line : HELP.drop.line)}`}
			>
				Drop
			</button>
		{/if}
	</div>
	{#if autoDrop}
		<!-- One line whatever its state, so the rail never grows during a countdown; below 820 px
		     tall the landmarks shorten and their note hides to make room for it -->
		<div
			class="mt-2 flex items-center gap-2 text-xs {autoDrop.state === 'counting'
				? 'border-l-2 border-violet-500 pl-2'
				: ''}"
			role={autoDrop.state === 'counting' ? 'alert' : undefined}
			title={autoDrop.title}
		>
			<span
				class="min-w-0 flex-1 truncate tabular-nums {autoDrop.state === 'counting' ||
				autoDrop.state === 'due'
					? 'font-semibold text-violet-400'
					: 'text-muted-foreground'}"
			>
				{autoDrop.text}
			</span>
			{#if autoDrop.state === 'counting'}
				<button
					class="shrink-0 rounded-md bg-red-600 px-2 py-0.5 font-medium text-white hover:bg-red-700 disabled:opacity-50"
					onclick={() => setAutoDrop(false)}
					disabled={autoDropBusy}>Cancel</button
				>
			{:else if autoDrop.state === 'armed'}
				<button
					class="{secondary} shrink-0 !py-0.5"
					onclick={() => setAutoDrop(false)}
					disabled={autoDropBusy}
					title="Cancel auto DROP for this roast: drop by hand"
				>
					Cancel
				</button>
			{:else if autoDrop.state === 'cancelled'}
				<button
					class="{secondary} shrink-0 !py-0.5"
					onclick={() => setAutoDrop(true)}
					disabled={autoDropBusy}>Turn back on</button
				>
			{/if}
		</div>
	{/if}
	{#if action.kind === 'start'}
		<Hint
			class="mt-2"
			help={{
				line: `Starts recording; the roaster waits. ${chargeSays ?? (virtual ? 'Then Charge loads the simulated beans.' : 'Charge when the beans go in.')}`
			}}
		/>
	{:else if action.kind === 'mark' && action.landmark === 'charge'}
		<Hint
			class="mt-2"
			help={chargeSays
				? { line: chargeSays, more: (virtual ? HELP.chargeVirtual : HELP.charge).line }
				: virtual
					? HELP.chargeVirtual
					: HELP.charge}
		/>
	{:else if action.kind === 'complete'}
		<Hint
			class="mt-2"
			help={{
				line: 'Saves the roast and switches the heater off. Weights, colour and cupping go on its page.'
			}}
		/>
	{/if}

	{#if s}
		<!-- Landmarks -->
		<div class="mt-3 grid grid-cols-4 gap-1.5">
			{#each LANDMARK_ORDER as type, i (type)}
				{@const ev = marked(type)}
				<button
					type="button"
					onclick={() => (ev ? unmark(ev) : markLandmark(type))}
					disabled={!ev && (paused || roast.busy)}
					title={ev
						? `Marked at ${fmtTemp(ev.temperature ?? null, 0)}${tempUnit()}: click to remove`
						: `Mark ${LANDMARK_ACTION[type]} (key ${i + 1})`}
					class="flex min-h-[2.75rem] flex-col items-center justify-center rounded-md border px-1 py-1 text-center text-[11px] leading-tight transition-colors disabled:opacity-40
						{autoDrop ? '[@media(max-height:820px)]:min-h-9 [@media(max-height:820px)]:py-0.5' : ''}
						{ev
						? 'border-green-500/30 bg-green-500/10 text-green-400'
						: action.kind === 'mark' && action.landmark === type
							? 'border-amber-500/60 text-foreground'
							: 'border-border text-muted-foreground hover:bg-accent hover:text-foreground'}"
				>
					<span class="font-medium">{LANDMARK_ACTION[type]}</span>
					{#if ev}
						<span class="tabular-nums"
							>{fmtClock(
								Math.max(
									0,
									ev.elapsed_seconds -
										(chargeEv && type !== 'charge' ? chargeEv.elapsed_seconds : 0)
								)
							)}</span
						>
					{:else}
						<kbd class="text-[9px] text-muted-foreground">{i + 1}</kbd>
					{/if}
				</button>
			{/each}
		</div>
		{#if marked('turning_point')}
			{@const tp = marked('turning_point')!}
			<p
				class="mt-1.5 text-[11px] text-muted-foreground {autoDrop
					? '[@media(max-height:820px)]:hidden'
					: ''}"
			>
				Turning point {fmtTemp(tp.temperature ?? null, 0)}{tempUnit()} at {fmtClock(
					Math.max(0, tp.elapsed_seconds - (chargeEv?.elapsed_seconds ?? 0))
				)} (auto)
			</p>
		{:else if chargeEv && marked('drying_end')}
			<p
				class="mt-1.5 text-[11px] text-muted-foreground {autoDrop
					? '[@media(max-height:820px)]:hidden'
					: ''}"
			>
				No turning point: BT rose from the charge without dipping (cold charge).
			</p>
		{/if}

		<!-- Secondary -->
		<div class="mt-2 flex flex-wrap gap-2">
			{#if paused}
				<button class={secondary} onclick={resumeRoast} disabled={roast.busy}
					><Play class="h-4 w-4" /> Resume</button
				>
			{:else}
				<button
					class={secondary}
					onclick={pauseRoast}
					disabled={roast.busy}
					title="Stop recording for a moment; the roaster carries on"
					><Pause class="h-4 w-4" /> Pause</button
				>
			{/if}
			<button
				class={secondary}
				onclick={() => (confirmCancel = true)}
				disabled={roast.busy || confirmCancel}
				title="Discard this roast: stops the drive, cuts heater power, cools a hot chamber, saves nothing"
			>
				<X class="h-4 w-4" /> Cancel
			</button>
			{#if action.kind !== 'complete'}
				<button class="{secondary} ml-auto" onclick={complete} disabled={roast.busy}
					><Check class="h-4 w-4" /> Complete</button
				>
			{/if}
		</div>
		{#if confirmCancel}
			<div
				class="mt-2 flex flex-wrap items-center gap-2 border-l-2 border-red-500 pl-2 text-xs"
				role="alert"
			>
				<span class="flex-1 text-foreground"
					>Discard this roast? Heater power goes off and nothing is saved.</span
				>
				<button
					class="rounded-md bg-red-600 px-2.5 py-1 font-medium text-white hover:bg-red-700 disabled:opacity-50"
					onclick={() => cancelRoast()}
					disabled={roast.busy}>Discard</button
				>
				<button class={secondary} onclick={() => (confirmCancel = false)}>Keep roasting</button>
			</div>
		{/if}
	{/if}
</section>
