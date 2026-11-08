<script lang="ts">
	/**
	 * One profile, read like an Artisan background (docs/profiles.md): its
	 * curves, landmarks, phases and control events, what it was made for,
	 * and Artisan export.
	 */
	import { resolve } from '$app/paths';
	import { goto } from '$app/navigation';
	import { downloadFile } from '#lib/api/http.js';
	import { profiles } from '#lib/api/profiles.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { roast, setPreviewProfile } from '#lib/stores/roast.svelte.js';
	import { fmtTemp, fmtDelta, tempUnit, rorUnit } from '#lib/stores/units.svelte.js';
	import { fmtClock } from '#lib/scope/scopeData.js';
	import { profileView, controlLabel, LANDMARK_LABEL } from '#lib/profile/view.js';
	import ProfileChart from '#lib/components/profiles/ProfileChart.svelte';
	import { levelLabel } from '#lib/guidance/references.js';
	import { planFromView, readPlan, reviewProfile, PHASE_HINT } from '#lib/guidance/profile.js';
	import { mmss } from '#lib/guidance/summary.js';
	import type { HistoryRoast } from '#lib/guidance/expect.js';
	import { batchLabel, fmtGrams } from '#lib/beans/stock.js';
	import { autoDropFact } from '#lib/replay/replay.js';
	import LevelStrip from '#lib/components/profiles/LevelStrip.svelte';
	import RangeBand from '#lib/components/ui/RangeBand.svelte';
	import GuidanceSummary from '#lib/components/history/GuidanceSummary.svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { purposeLabel } from '#lib/roast/properties.js';
	import { ArrowLeft } from '@lucide/svelte';
	import ProfileSharePanel from '#lib/components/profiles/ProfileSharePanel.svelte';
	import type { PageProps } from './$types';
	import { isOn } from '#lib/stores/modules.svelte.js';

	let { data }: PageProps = $props();
	const p = $derived(data.profile);
	const v = $derived(profileView(p));
	const mark = (code: string) => v.landmarks.find((l) => l.code === code) ?? null;

	const facts = $derived(
		[
			p.roast_level
				? `${levelLabel(p.roast_level)}${p.roast_purpose ? ` ${purposeLabel(p.roast_purpose).toLowerCase()}` : ''} roast`
				: p.roast_purpose
					? purposeLabel(p.roast_purpose)
					: null,
			p.beans,
			p.batch_size_g ? fmtGrams(p.batch_size_g) : null,
			p.roaster,
			autoDropFact(p, (c) => `${fmtTemp(c, 0)}${tempUnit()}`)
		].filter(Boolean) as string[]
	);

	// Roast guidance (docs/roast-guidance.md): the curve against its level, and how its roasts
	// on the selected roaster ran.
	const plan = $derived(planFromView(v));
	const planRead = $derived(readPlan(plan, p));
	let history = $state<HistoryRoast[]>([]);
	$effect(() => {
		const id = p.id;
		const dev = roast.deviceId ?? undefined;
		profiles
			.history(id, dev)
			.then((h) => (history = h))
			.catch(() => (history = []));
	});
	const review = $derived(reviewProfile(plan, p, history, (c) => `${fmtTemp(c, 1)} ${tempUnit()}`));
	const lvl = $derived(levelLabel(p.roast_level));
	const amber = (j: { state: string } | null) =>
		j && (j.state === 'above' || j.state === 'below') ? 'text-amber-400' : 'text-foreground';

	function useForNextRoast() {
		setPreviewProfile(p.id);
		goto(resolve('/'));
	}

	const fileBase = $derived(
		p.name.replace(/[^A-Za-z0-9._-]+/g, '_').replace(/^_+|_+$/g, '') || 'profile'
	);
	async function exportAs(kind: 'alog' | 'json') {
		try {
			if (kind === 'alog')
				await downloadFile(`/api/profiles/${p.id}/export/artisan`, `${fileBase}.alog`);
			else
				await downloadFile(`/api/profiles/${p.id}/export`, `${fileBase}.roastwarden-profile.json`);
		} catch (e) {
			notifications.add(`Export failed: ${e instanceof Error ? e.message : e}`, 'error');
		}
	}

	let confirmDelete = $state(false);
	let showShare = $state(false);
	async function remove() {
		try {
			await profiles.delete(p.id);
			if (roast.previewProfileId === p.id) setPreviewProfile('');
			goto(resolve('/profiles'));
		} catch (e) {
			notifications.add(`Failed to delete profile: ${e instanceof Error ? e.message : e}`, 'error');
		}
	}

	const SOURCE: Record<string, string> = {
		event: 'event',
		target: 'target',
		curve: 'from the curve'
	};
	function trigger(e: { trigger_kind: string; at_seconds: number | null; at_bt: number | null }) {
		return e.trigger_kind === 'bt' && e.at_bt != null
			? `at BT ${fmtTemp(e.at_bt, 0)}${tempUnit()}`
			: 'by time';
	}

	const btn =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent';
	const th = 'whitespace-nowrap px-3 py-1.5 text-left text-xs font-medium text-muted-foreground';
	const td = 'whitespace-nowrap px-3 py-1.5 tabular-nums';
</script>

<svelte:head>
	<title>{p.name} | Roastwarden</title>
</svelte:head>

<div class="space-y-4">
	<a
		href={resolve('/profiles')}
		class="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
	>
		<ArrowLeft class="h-4 w-4" /> Profiles
	</a>

	<!-- Header -->
	<div class="flex flex-wrap items-start justify-between gap-3">
		<div class="min-w-0">
			<h1 class="text-2xl font-bold text-foreground">{p.name}</h1>
			{#if facts.length}<p class="mt-0.5 text-sm text-foreground">{facts.join(' · ')}</p>{/if}
			{#if p.description}<p class="mt-1 max-w-3xl text-sm text-muted-foreground">
					{p.description}
				</p>{/if}
			{#if planRead.expected}
				<div
					class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground"
				>
					<LevelStrip level={p.roast_level} />
					<span>Expect · {planRead.expected}</span>
				</div>
			{:else}
				<p class="mt-1 text-xs text-muted-foreground">
					No roast level: <a
						href={resolve('/profiles/[id]/edit', { id: p.id })}
						class="underline underline-offset-2 hover:text-foreground">set one</a
					> to see what results to expect and to read its roasts against references.
				</p>
			{/if}
		</div>
		<div class="flex flex-wrap items-center gap-2">
			<button
				onclick={useForNextRoast}
				class="rounded-md bg-go px-3 py-1.5 text-sm font-medium text-go-foreground hover:bg-go-hover"
				title={roast.session
					? 'Picked for the roast after the current one'
					: 'Opens the Roast page with this profile picked'}
			>
				Use for next roast
			</button>
			<a href={resolve('/profiles/[id]/edit', { id: p.id })} class={btn}>Edit</a>
			{#if isOn('reports')}<a
					href={resolve(`/reports?kind=profile&profile=${encodeURIComponent(p.id)}`)}
					class={btn}
					title="This profile's report: its roasts ranked, overlaid and spread, as a PDF">Report</a
				>{/if}
			<button
				onclick={() => exportAs('alog')}
				class={btn}
				title="Opens in Artisan as a roast, or as a background (Roast → Background)"
				>Export .alog</button
			>
			<button
				onclick={() => exportAs('json')}
				class={btn}
				title="Roastwarden's own format, for another Roastwarden server">Export .json</button
			>
			<button
				onclick={() => (showShare = !showShare)}
				class={btn}
				aria-expanded={showShare}
				title="A public link: its page shows the curve and offers the profile to import"
				>Share</button
			>
			{#if confirmDelete}
				<button
					onclick={remove}
					class="rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700"
					>Delete for good</button
				>
				<button onclick={() => (confirmDelete = false)} class={btn}>Cancel</button>
			{:else}
				<button
					onclick={() => (confirmDelete = true)}
					class="rounded-md px-3 py-1.5 text-sm text-red-400 hover:bg-red-500/10">Delete</button
				>
			{/if}
		</div>
	</div>

	{#if showShare}
		<section class="max-w-3xl rounded-md border border-border bg-card">
			<h2 class="border-b border-border px-3 py-2 text-sm font-semibold text-foreground">
				Share this profile
			</h2>
			<div class="px-3 py-3"><ProfileSharePanel profileId={p.id} /></div>
		</section>
	{/if}

	{#if v.t.length < 2}
		<p class="text-sm text-muted-foreground">
			This profile has no points yet. <a
				href={resolve('/profiles/[id]/edit', { id: p.id })}
				class="text-foreground underline underline-offset-2">Open it in the designer</a
			>.
		</p>
	{:else}
		<!-- Key numbers, Artisan's statistics line -->
		<div
			class="grid grid-cols-2 gap-px overflow-hidden rounded-md border border-border bg-border text-sm sm:grid-cols-4 lg:grid-cols-7"
		>
			{#each [['CHARGE', 'Charge'], ['TP', 'Turning point'], ['DRY', 'Dry end'], ['FCs', 'First crack'], ['DROP', 'Drop']] as [code, label] (code)}
				{@const m = mark(code)}
				<div class="bg-card px-3 py-2">
					<div class="text-[11px] text-muted-foreground">{label}</div>
					{#if m}
						<div class="font-semibold tabular-nums text-foreground">
							{code === 'CHARGE' ? fmtTemp(m.bt, 0) + tempUnit() : fmtClock(m.t)}
						</div>
						{#if code !== 'CHARGE'}<div class="text-xs tabular-nums text-muted-foreground">
								{fmtTemp(m.bt, 1)}{tempUnit()}
							</div>{/if}
					{:else}
						<div class="text-muted-foreground">—</div>
					{/if}
				</div>
			{/each}
			<div class="bg-card px-3 py-2">
				<div class="text-[11px] text-muted-foreground">Development</div>
				<div class="font-semibold tabular-nums text-foreground">
					{v.dtr != null ? `${v.dtr.toFixed(1)} %` : '—'}
				</div>
				{#if planRead.development_s != null}
					<div class="text-xs tabular-nums text-muted-foreground">
						<RangeBand
							j={planRead.development}
							range={planRead.development
								? `${mmss(planRead.development.range[0])}–${mmss(planRead.development.range[1])} after first crack`
								: ''}
							level={lvl}>{mmss(planRead.development_s)} after FC</RangeBand
						>
					</div>
				{/if}
			</div>
			<div class="hidden bg-card px-3 py-2 sm:block">
				<div class="text-[11px] text-muted-foreground">Points · events</div>
				<div class="font-semibold tabular-nums text-foreground">
					{p.points.length} · {p.events?.length ?? 0}
				</div>
				<div class="text-xs text-muted-foreground">
					{p.curves ? 'with recorded roast' : 'designed'}
				</div>
			</div>
		</div>

		<div class="h-[360px] rounded-md border border-border bg-card p-2 sm:h-[460px]">
			<ProfileChart view={v} />
		</div>

		<div class="grid gap-4 lg:grid-cols-2">
			<!-- Phases -->
			<section class="min-w-0 rounded-md border border-border bg-card">
				<h2 class="border-b border-border px-3 py-2 text-sm font-semibold text-foreground">
					Phases
				</h2>
				{#if v.phases.length}
					<div class="overflow-x-auto">
						<table class="w-full text-sm">
							<thead
								><tr
									><th class={th}>Phase</th><th class="{th} text-right">Time</th><th
										class="{th} text-right"
										>Share <Hint
											inline
											help={{
												line: 'Information only: phase shares are not judged.',
												more: PHASE_HINT
											}}
										/></th
									><th class="{th} text-right">ΔBT ({tempUnit()})</th><th class="{th} text-right"
										>RoR ({rorUnit()})</th
									></tr
								></thead
							>
							<tbody class="divide-y divide-border">
								{#each v.phases as ph (ph.key)}
									<tr>
										<td class="{td} text-foreground">{ph.label}</td>
										<td class="{td} text-right">{fmtClock(ph.duration)}</td>
										<td class="{td} text-right"
											>{ph.pct != null ? `${ph.pct.toFixed(1)} %` : '—'}</td
										>
										<td class="{td} text-right">{ph.dT != null ? fmtDelta(ph.dT, 1, true) : '—'}</td
										>
										<td class="{td} text-right">{ph.ror != null ? fmtDelta(ph.ror) : '—'}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{:else}
					<p class="px-3 py-2 text-sm text-muted-foreground">
						Mark dry end and first crack (as events or targets) to see phases.
					</p>
				{/if}

				<h2 class="border-y border-border px-3 py-2 text-sm font-semibold text-foreground">
					Landmarks
				</h2>
				<div class="overflow-x-auto">
					<table class="w-full text-sm">
						<thead
							><tr
								><th class={th}>Landmark</th><th class="{th} text-right">Time</th><th
									class="{th} text-right">BT</th
								><th class="{th} text-right">ET</th><th class={th}>From</th></tr
							></thead
						>
						<tbody class="divide-y divide-border">
							{#each v.landmarks as l (l.code)}
								<tr>
									<td class="{td} text-foreground">{LANDMARK_LABEL[l.code]}</td>
									<td class="{td} text-right">{fmtClock(l.t)}</td>
									<td class="{td} text-right">{fmtTemp(l.bt, 1)}</td>
									<td class="{td} text-right">{l.et != null ? fmtTemp(l.et, 1) : '—'}</td>
									<td class="{td} text-xs text-muted-foreground">{SOURCE[l.source]}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</section>

			<!-- Events -->
			<section class="min-w-0 rounded-md border border-border bg-card">
				<h2 class="border-b border-border px-3 py-2 text-sm font-semibold text-foreground">
					Control events
				</h2>
				{#if v.controls.length}
					<div class="overflow-x-auto">
						<table class="w-full text-sm">
							<thead
								><tr><th class={th}>At</th><th class={th}>Does</th><th class={th}>Fires</th></tr
								></thead
							>
							<tbody class="divide-y divide-border">
								{#each v.controls as c, i (i)}
									<tr>
										<td class="{td} text-foreground">{fmtClock(c.t)}</td>
										<td class="{td} text-foreground"
											>{controlLabel(c, (x) => `${fmtTemp(x, 0)}${tempUnit()}`)}</td
										>
										<td class="{td} text-xs text-muted-foreground">{trigger(c.event)}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{:else}
					<p class="px-3 py-2 text-sm text-muted-foreground">
						No control events. {v.hasFan && v.hasHeater
							? 'Fan and heater follow'
							: v.hasFan
								? 'The fan follows'
								: v.hasHeater
									? 'The heater follows'
									: 'Nothing to follow in'} the points.
					</p>
				{/if}
				<p class="border-t border-border px-3 py-2 text-xs text-muted-foreground">
					In Artisan, fan and heater changes become Air and Burner events, which Artisan replays
					from a background profile. Setpoint, mode and note events export as labelled events.
				</p>
			</section>
		</div>

		<!-- How its roasts ran (roast guidance) -->
		<section class="min-w-0 rounded-md border border-border bg-card">
			<h2 class="border-b border-border px-3 py-2 text-sm font-semibold text-foreground">
				Roasts on {roast.deviceId ?? 'all roasters'}
				<span class="font-normal text-muted-foreground"
					>· {history.length ? `${history.length} completed` : 'none yet'}</span
				>
			</h2>
			{#if history.length}
				<GuidanceSummary lines={review.lines} class="px-3 pt-2" />
				<div class="overflow-x-auto">
					<table class="mt-1 w-full text-sm">
						<thead
							><tr
								><th class={th}>Roast</th><th class="{th} text-right">First crack</th><th
									class="{th} text-right">vs profile</th
								><th class="{th} text-right">Development</th><th class="{th} text-right">Loss</th
								><th class="{th} text-right">Colour</th></tr
							></thead
						>
						<tbody class="divide-y divide-border">
							{#each review.rows.slice(0, 10) as r (r.roast.session_id)}
								<tr>
									<td class="{td} text-foreground"
										><a
											href={resolve('/history/[id]', { id: r.roast.session_id })}
											class="hover:underline"
											>{#if r.roast.batch_number}<span
													class="mr-1 tabular-nums text-muted-foreground"
													>{batchLabel(r.roast.batch_number)}</span
												>{/if}{r.roast.name}</a
										>
										<span class="text-xs text-muted-foreground"
											>{new Date(r.roast.roasted_at).toLocaleDateString()}</span
										></td
									>
									<td class="{td} text-right tabular-nums"
										>{r.roast.fc_s != null
											? fmtClock(r.roast.fc_s)
											: '—'}{#if r.roast.fc_bt != null}<span
												class="ml-1.5 text-xs text-muted-foreground"
												>{fmtTemp(r.roast.fc_bt, 1)}</span
											>{/if}</td
									>
									<td class="{td} text-right tabular-nums"
										>{r.fcDelta != null
											? `${r.fcDelta < 0 ? '−' : '+'}${mmss(Math.abs(r.fcDelta))}`
											: '—'}</td
									>
									<td class="{td} text-right tabular-nums {amber(r.development)}"
										>{r.roast.development_s != null ? mmss(r.roast.development_s) : '—'}</td
									>
									<td class="{td} text-right tabular-nums {amber(r.loss)}"
										>{r.roast.weight_loss_pct != null
											? `${r.roast.weight_loss_pct.toFixed(1)} %`
											: '—'}</td
									>
									<td class="{td} text-right tabular-nums {amber(r.colour)}"
										>{r.roast.color_ground ?? '—'}</td
									>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				<p class="border-t border-border px-3 py-2 text-xs text-muted-foreground">
					Amber: outside what {lvl || 'the profile'} expects. Lines average the newest {Math.min(
						review.n,
						5
					)} roast{review.n === 1 ? '' : 's'}; first crack is compared with the profile’s own mark,
					never a published temperature.
				</p>
			{:else}
				<p class="px-3 py-2 text-sm text-muted-foreground">
					No completed roasts of this profile{roast.deviceId ? ' on this roaster' : ''} yet. After a few,
					this shows how they ran against the profile and what to edit.
				</p>
			{/if}
		</section>
	{/if}
</div>
