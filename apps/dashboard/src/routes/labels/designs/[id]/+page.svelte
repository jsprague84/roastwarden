<script lang="ts">
	/**
	 * A label design (docs/labels.md, "Label studio"): its Typst source in an editor, and
	 * beside it the live check the server runs on every change (a moment after typing stops):
	 * the label as it prints, what to fix, and the report exactly as an agent reads it. Saving
	 * makes a new version; versions compare side by side and roll back. A built-in or folder
	 * design is saved under a new id.
	 */
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import { track } from '#lib/stores/demo.svelte.js';
	import { goto, beforeNavigate } from '$app/navigation';
	import { labelStudio, type LabelCheck, type SavedDesign } from '#lib/api/labelStudio.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { kept, keep } from '#lib/stores/kept.js';
	import { changedParams, paramSpecs, startValues, type ParamSpec } from '#lib/labels/params.js';
	import { idFrom, previewCache, problemsOf, validId } from '#lib/labels/studio.js';
	import ParamFields from '#lib/components/labels/ParamFields.svelte';
	import TypstEditor from '#lib/components/labels/TypstEditor.svelte';
	import TypstDiff from '#lib/components/labels/TypstDiff.svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { untrack } from 'svelte';
	import { ArrowLeft, Check, TriangleAlert, CircleX, LoaderCircle } from '@lucide/svelte';

	let { data }: PageProps = $props();

	// The design as loaded (a saved one's versions update after a save)
	let design = $state<SavedDesign | null>(untrack(() => data.design));
	const kind = $derived(data.isNew ? 'new' : (design?.kind ?? 'saved'));

	let source = $state(untrack(() => data.source));
	// What was last saved or loaded: "unsaved" is a difference from it
	let savedSource = $state(untrack(() => (data.isNew ? '' : data.source)));
	const dirty = $derived(source !== savedSource);

	let name = $state(untrack(() => (data.isNew ? '' : (design?.name ?? ''))));
	let description = $state(untrack(() => design?.description ?? ''));
	// A built-in or folder design is kept under a new id: `my-cherry` to start
	let newId = $state(
		untrack(() => (!data.isNew && design && design.kind !== 'saved' ? `my-${design.id}` : ''))
	);
	let idTouched = $state(untrack(() => !data.isNew));
	let note = $state('');
	// A saved design with nothing changed since its last version: no version to save
	const unchanged = $derived(
		kind === 'saved' &&
			!!design &&
			!dirty &&
			name === design.name &&
			(description || '') === (design.description ?? '')
	);

	// What the check lays out: the sheet's label, sample data or a roast, settings, brand
	let sheet = $state(kept('labels.design.sheet', '5163'));
	// For coffee that is sold: the bag (the retail check)
	let retail = $state(kept('labels.design.retail', ''));
	$effect(() => keep('labels.design.retail', retail));
	let roast = $state('');
	let brand = $state('');
	let message = $state('');
	let specs = $state<ParamSpec[]>([]);
	let values = $state<Record<string, unknown>>({});
	const params = $derived(changedParams(specs, values));
	$effect(() => keep('labels.design.sheet', sheet));

	const sheets = $derived(data.options.sheets);
	const sheetInfo = $derived(sheets.find((s) => s.id === sheet) ?? sheets[0]);

	// ---- the live check ----
	let result = $state<LabelCheck | null>(null);
	let checking = $state(false);
	let failure = $state('');
	let view = $state<'label' | 'sheet' | 'report'>('label');
	let sheetSvg = $state('');

	const ask = $derived({
		source,
		sheet,
		roast: roast || undefined,
		params,
		brand: brand || undefined,
		message: message.trim() || undefined,
		retail: retail || undefined
	});

	// The last picture that compiled: kept, dimmed, while the source does not
	let lastGood = $state('');
	// Ctrl+Enter: check now, not after the pause
	let now = $state(0);
	let lastNow = 0;

	$effect(() => {
		const body = ask;
		const asked = now;
		const wait = asked !== lastNow ? 0 : 400;
		lastNow = asked;
		const controller = new AbortController();
		const t = setTimeout(async () => {
			checking = true;
			try {
				const c = await labelStudio.check({ ...body, png: 160 }, controller.signal);
				result = c;
				if (c.png) lastGood = c.png;
				failure = '';
				const manifest = c.report?.manifest;
				if (manifest !== undefined) {
					const next = paramSpecs(manifest);
					// New settings (a parameter added or removed in the source) reset the form
					if (
						JSON.stringify(next.map((s) => [s.key, s.default])) !==
						JSON.stringify(untrack(() => specs).map((s) => [s.key, s.default]))
					) {
						specs = next;
						values = startValues(
							next,
							untrack(() => values)
						);
					}
				}
			} catch (e) {
				if (!controller.signal.aborted) failure = e instanceof Error ? e.message : String(e);
			} finally {
				if (!controller.signal.aborted) checking = false;
			}
		}, wait);
		return () => {
			clearTimeout(t);
			controller.abort();
		};
	});

	// The whole sheet, only while it is shown
	$effect(() => {
		if (view !== 'sheet') return;
		const body = ask;
		const controller = new AbortController();
		const t = setTimeout(async () => {
			try {
				sheetSvg = await labelStudio.renderSvg(
					{ ...body, count: sheetInfo.cols * sheetInfo.rows },
					controller.signal
				);
			} catch {
				if (!controller.signal.aborted) sheetSvg = '';
			}
		}, 500);
		return () => {
			clearTimeout(t);
			controller.abort();
		};
	});

	const errors = $derived(result?.errors ?? []);
	const report = $derived(result?.report);
	const issues = $derived(report?.issues ?? []);
	// Soft issues (a field left off, a design's hint) leave the label clean
	const hard = $derived(issues.filter((i) => i.kind !== 'dropped' && i.kind !== 'hint'));
	const problems = $derived(problemsOf(errors));
	const status = $derived(
		failure
			? 'failed'
			: errors.length
				? 'errors'
				: !result
					? 'waiting'
					: hard.length
						? 'issues'
						: 'clean'
	);
	let editor: TypstEditor | undefined = $state();

	// ---- saving ----
	let saving = $state(false);
	$effect(() => {
		if (data.isNew && !idTouched) newId = idFrom(name);
	});

	async function save() {
		if (saving) return;
		if (kind === 'saved' && design) {
			if (!dirty && name === design.name && (description || '') === (design.description ?? '')) {
				notifications.add('Nothing changed', 'warning');
				return;
			}
			saving = true;
			try {
				const r = await labelStudio.edit(design.id, {
					source: dirty ? source : undefined,
					note: note.trim() || undefined,
					name: name.trim() && name !== design.name ? name.trim() : undefined,
					description: (description || '') !== (design.description ?? '') ? description : undefined,
					sheet
				});
				design = r.design;
				track('Label designed');
				savedSource = r.design.source;
				note = '';
				notifications.add(`Saved ${design.id}@${design.version}`, 'success');
				for (const k of Object.keys(previewCache))
					if (k.startsWith(`${design.id}@`)) delete previewCache[k];
			} catch (e) {
				notifications.add(`Save: ${e instanceof Error ? e.message : String(e)}`, 'error');
			} finally {
				saving = false;
			}
			return;
		}
		// A new design, or a built-in / folder one kept under a new id
		const id = (data.isNew ? newId : newId || idFrom(name)).trim();
		if (!validId(id) || id === 'new') {
			notifications.add('Give it an id: letters, digits, - and _', 'error');
			return;
		}
		saving = true;
		try {
			const r = await labelStudio.create({
				id,
				name: name.trim() || undefined,
				description: description.trim() || undefined,
				source,
				note: note.trim() || (data.isNew ? undefined : `from ${design?.id ?? data.from}`),
				sheet
			});
			savedSource = source;
			track('Label designed');
			notifications.add(`Saved ${r.design.id}@${r.design.version}`, 'success');
			await goto(resolve('/labels/designs/[id]', { id: encodeURIComponent(r.design.id) }), {
				replaceState: true,
				invalidateAll: true
			});
		} catch (e) {
			notifications.add(`Save: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			saving = false;
		}
	}

	// A new page for the same route (after "Save as") starts from its data
	$effect(() => {
		const d = data.design;
		untrack(() => {
			if (d && d.id !== design?.id) {
				design = d;
				source = d.source;
				savedSource = d.source;
				name = d.name;
				description = d.description ?? '';
			}
		});
	});

	beforeNavigate(({ cancel, to }) => {
		if (dirty && to && !confirm('Leave without saving this design?')) cancel();
	});

	// ---- versions ----
	let compare = $state<{ version: number; source: string } | null>(null);

	async function compareWith(version: number) {
		if (!design) return;
		try {
			const v = await labelStudio.design(`${design.id}@${version}`);
			compare = { version, source: v.source };
		} catch (e) {
			notifications.add(
				`Version ${version}: ${e instanceof Error ? e.message : String(e)}`,
				'error'
			);
		}
	}

	async function openVersion(version: number) {
		if (!design) return;
		if (dirty && !confirm('Replace your unsaved changes with this version?')) return;
		try {
			const v = await labelStudio.design(`${design.id}@${version}`);
			source = v.source;
			notifications.add(
				`Version ${version} is in the editor: save to make it the current one`,
				'success'
			);
		} catch (e) {
			notifications.add(
				`Version ${version}: ${e instanceof Error ? e.message : String(e)}`,
				'error'
			);
		}
	}

	async function rollback(version: number) {
		if (!design) return;
		if (dirty && !confirm('Your unsaved changes are lost. Go back to this version?')) return;
		try {
			await labelStudio.rollback(design.id, version);
			design = await labelStudio.design(design.id);
			source = design.source;
			savedSource = design.source;
			notifications.add(
				`${design.name}: version ${version} is now version ${design.version}`,
				'success'
			);
		} catch (e) {
			notifications.add(
				`Back to ${version}: ${e instanceof Error ? e.message : String(e)}`,
				'error'
			);
		}
	}

	async function remove() {
		if (!design || !confirm(`Delete ${design.name} and all its versions?`)) return;
		try {
			await labelStudio.remove(design.id);
			savedSource = source;
			notifications.add(`Deleted ${design.name}`, 'success');
			await goto(resolve('/labels'));
		} catch (e) {
			notifications.add(`Delete: ${e instanceof Error ? e.message : String(e)}`, 'error');
		}
	}

	const kindLabel: Record<string, string> = {
		saved: 'saved',
		folder: 'from the labels folder',
		builtin: 'built in',
		new: 'new'
	};
	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const lbl = 'block text-xs font-medium text-muted-foreground';
	const shapeRatio = $derived(sheetInfo ? `${sheetInfo.w} / ${sheetInfo.h}` : '2 / 1');
	const fmt = (n: number) => n.toFixed(1);
</script>

<svelte:head>
	<title>{data.isNew ? 'New design' : (design?.name ?? 'Design')} · Labels | Roastwarden</title>
</svelte:head>

<!-- Header: what this is, and saving it -->
<div class="flex flex-wrap items-end gap-x-4 gap-y-2">
	<a
		href={resolve('/labels')}
		class="inline-flex items-center gap-1 pb-1.5 text-sm text-muted-foreground hover:text-foreground"
		><ArrowLeft class="h-4 w-4" /> Designs</a
	>
	<label class="min-w-48 flex-1 {lbl}"
		>Name
		<input
			class="{field} text-base font-semibold"
			bind:value={name}
			placeholder={data.isNew ? 'Sam’s gift' : ''}
		/>
	</label>
	{#if kind !== 'saved'}
		<label class="w-44 {lbl}"
			>Id
			<input
				class="{field} font-mono"
				bind:value={newId}
				oninput={() => (idTouched = true)}
				placeholder={idFrom(name) || 'sams-gift'}
			/>
		</label>
	{/if}
	<label class="min-w-48 flex-[2] {lbl}"
		>Note for this version
		<input class={field} bind:value={note} placeholder="What changed (kept with the version)" />
	</label>
	<div class="flex items-center gap-2 pb-0.5">
		<button
			onclick={save}
			disabled={saving || unchanged}
			class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
			title={unchanged ? `Nothing changed since version ${design?.version}` : 'Ctrl+S'}
		>
			{#if saving}Saving…{:else if kind === 'saved'}Save version {(design?.version ?? 0) +
					1}{:else if kind === 'new'}Save design{:else}Save as a new design{/if}
		</button>
		{#if dirty}<span class="text-xs text-highlight">unsaved</span>{/if}
	</div>
</div>
<p class="mt-1 text-xs text-muted-foreground">
	{#if design && kind === 'saved'}
		<span class="font-mono">{design.id}@{design.version}</span> · saved · {design.versions.length} version{design
			.versions.length === 1
			? ''
			: 's'}
	{:else if kind === 'builtin' || kind === 'folder'}
		<span class="font-mono">{design?.id}</span> · {kindLabel[kind]}: edit freely, then save it under
		a new id to keep it
	{:else}
		New: {data.from ? `starting from ${data.from}` : 'starting from the template'}; it is saved only
		when it compiles
	{/if}
</p>

<div class="mt-4 grid gap-5 xl:grid-cols-[minmax(0,1.05fr)_minmax(0,1fr)]">
	<!-- The source -->
	<section class="min-w-0">
		<div class="h-[calc(100vh-15rem)] min-h-[28rem]">
			<TypstEditor
				bind:this={editor}
				bind:value={source}
				{problems}
				onsave={save}
				oncheck={() => now++}
			/>
		</div>
		<p class="mt-1.5 text-xs text-muted-foreground">
			The server checks a moment after you stop typing (Ctrl+Enter: now). Ctrl+S saves. Ctrl+Space
			completes Typst's functions.
		</p>
		<label class="mt-3 block {lbl}"
			>Description
			<input
				class={field}
				bind:value={description}
				placeholder="What it is for (the gallery shows it)"
			/>
		</label>
	</section>

	<!-- The live check -->
	<section class="min-w-0 space-y-4">
		<div class="flex flex-wrap items-end gap-3">
			<label class={lbl}
				>Label
				<select class={field} bind:value={sheet}>
					{#each sheets as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
				</select>
			</label>
			<label class="min-w-40 flex-1 {lbl}"
				>Data
				<select class={field} bind:value={roast}>
					<option value="">Sample coffee</option>
					{#each data.roasts as r (r.id)}<option value={r.id}
							>{r.batch_number ? `#${r.batch_number} ` : ''}{r.name}</option
						>{/each}
				</select>
			</label>
			{#if data.brands.length}
				<label class={lbl}
					>Brand
					<select class={field} bind:value={brand}>
						<option value="">None</option>
						{#each data.brands as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
					</select>
				</label>
			{/if}
			<label class="min-w-32 flex-1 {lbl}"
				>Message
				<input class={field} bind:value={message} placeholder="for Sam" />
			</label>
			<label class={lbl}
				>For sale on
				<select
					class={field}
					bind:value={retail}
					title="The retail check: what a bag that is sold needs (21 CFR 101.3, 101.5, 101.7)"
				>
					<option value="">Not for sale</option>
					{#each data.options.bags ?? [] as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
					<option value="label">The label is the whole front</option>
				</select>
			</label>
		</div>
		{#if specs.length}
			<details open class="rounded-md border border-border bg-card px-3 py-2">
				<summary class="cursor-pointer text-sm font-medium text-foreground"
					>Settings <span class="text-xs font-normal text-muted-foreground"
						>(the design's params)</span
					></summary
				>
				<div class="mt-2 grid gap-3 sm:grid-cols-2"><ParamFields {specs} bind:values /></div>
			</details>
		{/if}

		<!-- Status -->
		<div class="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-border text-sm">
			<span class="inline-flex items-center gap-1.5 pb-1.5 font-medium" aria-live="polite">
				{#if checking}<LoaderCircle class="h-4 w-4 animate-spin text-muted-foreground" />{/if}
				{#if status === 'clean'}<Check class="h-4 w-4 text-go" /> Clean: nothing to fix{#if issues.length},
						{issues.length === 1 ? 'a note' : `${issues.length} notes`}{/if}
				{:else if status === 'issues'}<TriangleAlert class="h-4 w-4 text-amber-500" />
					{hard.length} to look at
				{:else if status === 'errors'}<CircleX class="h-4 w-4 text-destructive" /> Does not compile
				{:else if status === 'failed'}<CircleX class="h-4 w-4 text-destructive" /> Check failed
				{:else}Checking…{/if}
			</span>
			<span class="ml-auto flex gap-3">
				{#each [['label', 'Label'], ['sheet', 'On the sheet'], ['report', 'As an agent sees it']] as [k, t] (k)}
					<button
						class="-mb-px border-b-2 pb-1.5 {view === k
							? 'border-action text-foreground'
							: 'border-transparent text-muted-foreground hover:text-foreground'}"
						onclick={() => (view = k as typeof view)}>{t}</button
					>
				{/each}
			</span>
		</div>

		{#if failure}
			<p class="text-sm text-destructive">{failure}</p>
		{/if}

		{#if errors.length}
			<ul class="space-y-1.5">
				{#each errors as e, i (i)}
					<li class="rounded-md border border-destructive/50 bg-card px-3 py-2 text-sm">
						{#if e.line && (e.file === 'design.typ' || !e.file)}
							<button
								class="font-mono text-xs text-action hover:underline"
								onclick={() => editor?.goto(e.line!)}
								>line {e.line}{e.column ? `:${e.column}` : ''}</button
							>
						{:else if e.file}
							<span class="font-mono text-xs text-muted-foreground"
								>{e.file}{e.line ? `:${e.line}` : ''}</span
							>
						{/if}
						<span class="text-foreground">{e.message}</span>
						{#each e.hints ?? [] as h (h)}<p class="mt-0.5 text-xs text-muted-foreground">
								hint: {h}
							</p>{/each}
					</li>
				{/each}
			</ul>
		{/if}

		{#if view === 'label'}
			<div class="relative mx-auto max-w-2xl {checking ? 'opacity-80' : ''}">
				<div
					class="flex items-center justify-center overflow-hidden {sheetInfo?.round
						? 'rounded-full'
						: 'rounded-sm'} bg-white shadow"
					style="aspect-ratio: {shapeRatio}"
				>
					{#if result?.png}
						<img
							src="data:image/png;base64,{result.png}"
							alt="The label as it prints"
							class="h-full w-full object-contain"
						/>
					{:else if lastGood}
						<img
							src="data:image/png;base64,{lastGood}"
							alt="The label as it last compiled"
							class="h-full w-full object-contain opacity-35 grayscale"
						/>
					{:else}
						<span class="text-xs text-neutral-500"
							>{errors.length ? 'Fix the errors to see it' : '…'}</span
						>
					{/if}
				</div>
				{#if !result?.png && lastGood && errors.length}
					<span
						class="absolute left-2 top-2 rounded-sm bg-destructive px-1.5 py-0.5 text-xs font-medium text-white"
						>out of date: the last version that compiled</span
					>
				{/if}
				<p class="mt-1 text-center text-xs text-muted-foreground">
					{sheetInfo?.name}: the label as the server lays it out (dashed: its edge; nothing outside
					prints)
				</p>
			</div>
		{:else if view === 'sheet'}
			<div
				class="relative mx-auto w-full max-w-md bg-white shadow"
				style="aspect-ratio: {sheetInfo.page_w} / {sheetInfo.page_h}"
			>
				{#if sheetSvg}<div class="absolute inset-0 [&>svg]:h-full [&>svg]:w-full">
						<!-- eslint-disable-next-line svelte/no-at-html-tags -- the server's own label render (Typst → SVG) -->
						{@html sheetSvg}
					</div>{/if}
			</div>
			<p class="text-center text-xs text-muted-foreground">
				A whole {sheetInfo.name} sheet, every label filled
			</p>
		{:else}
			<pre
				class="max-h-[34rem] overflow-auto rounded-md border border-border bg-card p-3 text-xs text-foreground">{JSON.stringify(
					{ ok: result?.ok, errors: errors.length ? errors : undefined, report },
					null,
					1
				)}</pre>
			<p class="text-xs text-muted-foreground">
				What <code>roast labels design check</code> answers: an agent fixes a design from this, and looks
				at a picture once at the end.
			</p>
		{/if}

		{#if issues.length && view !== 'report'}
			<div>
				<h3 class="text-sm font-semibold text-foreground">To look at</h3>
				<ul class="mt-1 space-y-1 text-sm">
					{#each issues as is, i (i)}
						<li class="flex gap-2">
							<span class="w-32 shrink-0 break-words font-mono text-xs text-muted-foreground"
								>{is.kind}</span
							><span class="min-w-0 text-foreground">{is.message}</span>
						</li>
					{/each}
				</ul>
			</div>
		{/if}

		{#if result?.retail && view !== 'report'}
			<div class="rounded-md border border-border bg-card px-3 py-2">
				<h3 class="flex items-center gap-2 text-sm font-semibold text-foreground">
					For sale: {result.retail.ok ? 'nothing to fix' : `${result.retail.issues.length} to fix`}
					<Hint
						inline
						help={{
							line: `On ${result.retail.bag}: the net weight's letters at least ${result.retail.min_net_mm} mm.`,
							more: 'Read against 21 CFR 101.3 (what it is), 101.7 (the net weight) and 101.5 (who packed it, where). The type height follows the bag’s front, not the label. A reading of the rules, not legal advice.'
						}}
					/>
				</h3>
				{#if result.retail.passed.length}
					<ul class="mt-1 text-xs text-muted-foreground">
						{#each result.retail.passed as p (p)}<li>✓ {p}</li>{/each}
					</ul>
				{/if}
				<details class="mt-1 text-xs">
					<summary class="cursor-pointer text-muted-foreground">What no check can know</summary>
					<ul class="mt-1 list-disc pl-5 text-muted-foreground">
						{#each result.retail.cannot_check as c (c)}<li>{c}</li>{/each}
					</ul>
				</details>
			</div>
		{/if}

		{#if report && view === 'label'}
			<details class="rounded-md border border-border bg-card px-3 py-2">
				<summary class="cursor-pointer text-sm font-medium text-foreground">
					Lines of text <span class="text-xs font-normal text-muted-foreground"
						>{report.lines.length} · fields printed {report.fields.shown.length}{report.fields
							.missing.length
							? `, not printed: ${report.fields.missing.join(', ')}`
							: ''}</span
					>
				</summary>
				<table class="mt-2 w-full text-xs">
					<thead
						><tr class="text-left text-muted-foreground"
							><th class="py-0.5 font-medium">Text</th><th class="font-medium">Font</th><th
								class="text-right font-medium">pt</th
							><th class="text-right font-medium">x, y mm</th><th class="text-right font-medium"
								>w × h mm</th
							></tr
						></thead
					>
					<tbody>
						{#each report.lines as l, i (i)}
							<tr class="border-t border-border"
								><td class="max-w-[14rem] truncate py-0.5 text-foreground" title={l.text}
									>{l.text}</td
								><td class="truncate text-muted-foreground">{l.font}</td><td
									class="text-right tabular-nums">{fmt(l.pt)}</td
								><td class="text-right tabular-nums">{fmt(l.x)}, {fmt(l.y)}</td><td
									class="text-right tabular-nums">{fmt(l.w)} × {fmt(l.h)}</td
								></tr
							>
						{/each}
					</tbody>
				</table>
			</details>
		{/if}

		<!-- Versions -->
		{#if design && kind === 'saved'}
			<div>
				<h3 class="flex items-center gap-2 text-sm font-semibold text-foreground">
					Versions
					<Hint
						inline
						help={{
							line: 'Each save is a new version; a reprint can name one (sam-gift@2).',
							more: '“Back to this” copies that version’s source into a new version, so nothing is lost. Compare shows a version beside what is in the editor now.'
						}}
					/>
				</h3>
				<table class="mt-1 w-full text-sm">
					<tbody>
						{#each design.versions as v (v.version)}
							<tr class="border-t border-border">
								<td class="py-1 pr-2 tabular-nums"
									>{v.version}{v.version === design.version ? ' · current' : ''}</td
								>
								<td class="pr-2 text-xs text-muted-foreground"
									>{new Date(v.created_at).toLocaleString()} · {v.created_by}</td
								>
								<td
									class="max-w-[12rem] truncate pr-2 text-xs text-muted-foreground"
									title={v.note ?? ''}>{v.note ?? ''}</td
								>
								<td class="whitespace-nowrap text-right text-xs">
									<button class="text-action hover:underline" onclick={() => compareWith(v.version)}
										>Compare</button
									>
									{#if v.version !== design.version}
										· <button
											class="text-action hover:underline"
											onclick={() => openVersion(v.version)}>Open</button
										>
										·
										<button class="text-action hover:underline" onclick={() => rollback(v.version)}
											>Back to this</button
										>
									{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
				{#if compare}
					<div class="mt-3">
						<div class="mb-1 flex items-center justify-between text-xs">
							<span class="text-muted-foreground">Version {compare.version} beside the editor</span>
							<button
								class="text-muted-foreground hover:text-foreground"
								onclick={() => (compare = null)}>Close</button
							>
						</div>
						<TypstDiff
							a={compare.source}
							b={source}
							aLabel="version {compare.version}"
							bLabel={dirty ? 'the editor (unsaved)' : `version ${design.version}`}
						/>
					</div>
				{/if}
				<div class="mt-3 flex flex-wrap gap-3 text-xs">
					{#if design.id}<a class="text-action hover:underline" href={resolve('/labels/coffees')}
							>Give a coffee this label</a
						>{/if}
					<button class="ml-auto text-muted-foreground hover:text-destructive" onclick={remove}
						>Delete this design</button
					>
				</div>
			</div>
		{/if}
	</section>
</div>
