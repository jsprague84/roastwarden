<script lang="ts">
	/**
	 * Label Studio → Print (docs/labels.md, "Print runs"): one sheet of labels for one roast
	 * or several at once (a gift run). Each roast keeps its coffee's design unless told, its
	 * own copies and message, and its own QR code; the sheet starts at its next free label.
	 * Each roast's labels use the same editor as a roast's Labels page. Download the PDF
	 * (kept in History) or print it on the label printer, then "Printed it" here.
	 */
	import type { PageProps } from './$types';
	import { labelStudio } from '#lib/api/labelStudio.js';
	import { labels as labelsApi } from '#lib/api/labels.js';
	import { kept, keep } from '#lib/stores/kept.js';
	import {
		cellOwners,
		labelCount,
		newItem,
		nextAfter,
		roastMatches,
		runBody,
		sheetsFor,
		type RunItemAsk
	} from '#lib/labels/runs.js';
	import { slotBoxes, type LabelOptions } from '#lib/labels/labels.js';
	import LabelItemFields from '#lib/components/labels/LabelItemFields.svelte';
	import RunActions from '#lib/components/labels/RunActions.svelte';
	import PrinterPick from '#lib/components/labels/PrinterPick.svelte';
	import type { LabelPrinter } from '#lib/api/labelStudio.js';
	import { untrack } from 'svelte';
	import { ArrowDown, ArrowUp, X } from '@lucide/svelte';

	let { data }: PageProps = $props();

	const beanOf = (beanId?: string | null) => data.beans.find((b) => b.id === beanId);
	const roastOf = (id: string) => data.roasts.find((r) => r.id === id);
	// The sheets' next free labels: reloaded after a print or "Printed it"
	let options = $state<LabelOptions>(untrack(() => data.options));
	const roastName = (r: { name: string; batch_number?: number | null }) =>
		`${r.batch_number ? `#${r.batch_number} ` : ''}${r.name}`;

	// The roasts on the sheet, in order
	let items = $state<RunItemAsk[]>(
		untrack(() =>
			data.asked
				.map((id) => data.roasts.find((r) => r.id === id))
				.filter((r) => r !== undefined)
				.map((r) => newItem(r.id, roastName(r)))
		)
	);

	let sheetId = $state(
		kept(
			'labels.print.sheet',
			untrack(() => data.options.sheets[0]?.id ?? '5163')
		)
	);
	$effect(() => keep('labels.print.sheet', sheetId));
	const sheet = $derived(options.sheets.find((s) => s.id === sheetId) ?? options.sheets[0]);
	const per = $derived(sheet.cols * sheet.rows);
	// The sheet's next free label, unless one is clicked
	let chosenStart = $state<number | null>(null);
	const start = $derived(Math.min(chosenStart ?? sheet.next_label ?? 1, per));
	$effect(() => {
		void sheetId;
		chosenStart = null;
	});
	let note = $state('');
	let shared = $state('');
	// The printer (its calibration lays the sheet out; the print goes there), tray, colour
	let printerId = $state('');
	let tray = $state('');
	let color = $state('');
	let printerList = $state<LabelPrinter[]>([]);

	const total = $derived(labelCount(items));
	const owners = $derived(cellOwners(items, start, per));
	const pages = $derived(sheetsFor(total, start, per));

	// Adding roasts: recent finished ones, searchable; open until closed, to pick several
	let adding = $state(untrack(() => !items.length));
	let find = $state('');
	const choices = $derived(
		data.roasts.filter(
			(r) =>
				!items.some((i) => i.roast === r.id) &&
				roastMatches(find, roastName(r), beanOf(r.bean_id)?.name)
		)
	);
	function add(r: (typeof data.roasts)[number]) {
		items = [...items, newItem(r.id, roastName(r), { message: shared })];
	}
	function move(i: number, by: number) {
		const j = i + by;
		if (j < 0 || j >= items.length) return;
		const next = [...items];
		[next[i], next[j]] = [next[j], next[i]];
		items = next;
	}
	function applyMessage() {
		items = items.map((i) => ({ ...i, message: shared }));
	}

	// The whole first sheet as the server lays it out, a moment after the last change
	let svg = $state('');
	let problem = $state('');
	let loading = $state(false);
	// What the QR codes open (docs/labels.md, "The share page"): the server's default first
	let qr = $state(untrack(() => data.options.qr) ?? 'app');
	const body = $derived(runBody(items, { sheet: sheetId, start, note, printer: printerId, qr }));
	$effect(() => {
		const b = body;
		if (!b.items.length) {
			svg = '';
			problem = '';
			return;
		}
		const controller = new AbortController();
		const t = setTimeout(async () => {
			loading = true;
			try {
				svg = await labelStudio.previewRun(b, controller.signal);
				problem = '';
			} catch (e) {
				if (!controller.signal.aborted) problem = e instanceof Error ? e.message : String(e);
			} finally {
				if (!controller.signal.aborted) loading = false;
			}
		}, 350);
		return () => {
			clearTimeout(t);
			controller.abort();
		};
	});

	async function reloadSheets() {
		options = await labelsApi.options();
		chosenStart = null;
	}

	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const lbl = 'block text-xs font-medium text-muted-foreground';
	const used = $derived(
		start > 1 ? (start === 2 ? 'label 1 used' : `labels 1–${start - 1} used`) : 'a new sheet'
	);
</script>

<div class="grid gap-6 pb-24 lg:grid-cols-[minmax(0,30rem)_minmax(0,1fr)] lg:pb-0">
	<section class="min-w-0 space-y-4">
		<div>
			<h2 class="text-base font-semibold text-foreground">On this sheet</h2>
			<p class="text-sm text-muted-foreground">
				Each roast keeps its coffee's own design unless you pick one, and gets its own QR code.
			</p>
		</div>
		{#if items.length}
			<ol class="space-y-2">
				{#each items as it, i (it.roast)}
					<li class="rounded-md border border-border bg-card p-3">
						<div class="flex items-start gap-2">
							<span
								class="mt-0.5 inline-flex h-5 min-w-5 items-center justify-center rounded-sm bg-selected px-1 text-xs font-semibold text-selected-foreground tabular-nums"
								>{i + 1}</span
							>
							<span
								class="min-w-0 flex-1 truncate text-sm font-medium text-foreground"
								title={it.name}>{it.name}</span
							>
							<button
								class="text-muted-foreground hover:text-foreground disabled:opacity-30"
								disabled={i === 0}
								onclick={() => move(i, -1)}
								aria-label="Up"><ArrowUp class="h-4 w-4" /></button
							>
							<button
								class="text-muted-foreground hover:text-foreground disabled:opacity-30"
								disabled={i === items.length - 1}
								onclick={() => move(i, 1)}
								aria-label="Down"><ArrowDown class="h-4 w-4" /></button
							>
							<button
								class="text-muted-foreground hover:text-destructive"
								onclick={() => (items = items.filter((_, k) => k !== i))}
								aria-label="Take it off"><X class="h-4 w-4" /></button
							>
						</div>
						<div class="mt-2">
							<LabelItemFields
								bind:item={items[i]}
								compact
								designs={options.designs}
								brands={data.brands}
								fields={options.fields}
								sheet={sheetId}
								own={beanOf(roastOf(it.roast)?.bean_id)}
								roastedWeight={roastOf(it.roast)?.roasted_weight}
							/>
						</div>
					</li>
				{/each}
			</ol>
		{:else}
			<p class="rounded-md border border-dashed border-border p-4 text-sm text-muted-foreground">
				No roasts yet: add them below.
			</p>
		{/if}

		<details bind:open={adding} class="rounded-md border border-border bg-card px-3 py-2">
			<summary class="cursor-pointer text-sm font-medium text-foreground">Add roasts</summary>
			<input
				type="search"
				class="{field} mt-2"
				bind:value={find}
				placeholder="Find a roast or coffee"
			/>
			<ul class="mt-2 max-h-64 divide-y divide-border overflow-auto">
				{#each choices.slice(0, 40) as r (r.id)}
					<li>
						<button
							class="flex w-full items-baseline gap-2 px-1 py-1.5 text-left text-sm hover:bg-accent"
							onclick={() => add(r)}
						>
							<span class="truncate text-foreground">{roastName(r)}</span>
							{#if beanOf(r.bean_id) && !r.name
									.toLowerCase()
									.includes(beanOf(r.bean_id)!.name.toLowerCase())}
								<span class="truncate text-xs text-muted-foreground">{beanOf(r.bean_id)!.name}</span
								>
							{/if}
							<span class="ml-auto shrink-0 text-xs text-muted-foreground"
								>{r.end_time ? new Date(r.end_time).toLocaleDateString() : ''}</span
							>
						</button>
					</li>
				{:else}
					<li class="py-2 text-sm text-muted-foreground">No other finished roast matches.</li>
				{/each}
			</ul>
		</details>

		<div class="grid grid-cols-2 gap-3">
			<label class={lbl}
				>Label sheet
				<select class={field} bind:value={sheetId}>
					{#each options.sheets as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
				</select>
			</label>
			<label class={lbl}
				>Start at label
				<input
					class={field}
					type="number"
					min="1"
					max={per}
					value={start}
					oninput={(e) => (chosenStart = Number(e.currentTarget.value) || 1)}
				/>
			</label>
		</div>
		<p class="-mt-2 text-xs text-muted-foreground">
			Load {sheet.name.split(',')[0]}, {used}{chosenStart === null && (sheet.next_label ?? 1) > 1
				? ' (as the last print left it)'
				: ''}.
			{#if start > 1}<button class="text-action hover:underline" onclick={() => (chosenStart = 1)}
					>A new sheet</button
				>{/if}
			Click a label in the picture to start there.
		</p>

		<div class="grid grid-cols-[minmax(0,1fr)_auto] items-end gap-2">
			<label class={lbl}
				>One message for all
				<input class={field} bind:value={shared} placeholder="Happy holidays" />
			</label>
			<button
				class="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent"
				onclick={applyMessage}
				disabled={!items.length}>Use</button
			>
		</div>
		<label class={lbl}
			>QR codes open
			<select class={field} bind:value={qr}>
				<option value="app">Each roast in Roastwarden (your login)</option>
				<option value="share">Each roast's share page (anyone with the bag)</option>
				<option value="supplier">Each coffee at its supplier</option>
				<option value="none">No QR codes</option>
			</select>
		</label>
		<label class={lbl}
			>Note (kept with the run)
			<input class={field} bind:value={note} placeholder="gift baskets" />
		</label>
		<PrinterPick bind:printer={printerId} bind:tray bind:color bind:printers={printerList} />
		<p class="text-xs text-muted-foreground">
			{total} label{total === 1 ? '' : 's'} on {pages === 1 ? 'one sheet' : `${pages} sheets`};
			after it, the next free label is {nextAfter(start, total, per)}.
		</p>
	</section>

	<section class="min-w-0">
		<RunActions
			ask={() => body}
			disabled={!items.length || !!problem}
			why={items.length ? problem : 'Add a roast first'}
			{per}
			{printerId}
			{tray}
			{color}
			loadHint={`Load a ${sheet.id} sheet (${used}) in tray 1 first`}
			onsheets={reloadSheets}
		/>
		{#if problem}
			<pre
				class="mb-3 whitespace-pre-wrap rounded-md border border-destructive/50 bg-card p-3 text-sm text-foreground">{problem}</pre>
		{/if}
		<div
			class="relative mx-auto w-full max-w-[40rem] bg-white shadow {loading ? 'opacity-80' : ''}"
			style="aspect-ratio: {sheet.page_w} / {sheet.page_h}"
		>
			<!-- eslint-disable-next-line svelte/no-at-html-tags -- the server's own label render (Typst → SVG) -->
			{#if svg}<div class="absolute inset-0 [&>svg]:h-full [&>svg]:w-full">{@html svg}</div>{/if}
			{#each slotBoxes(sheet) as b, i (i)}
				<button
					class="absolute rounded-[3%] hover:outline hover:outline-2 hover:outline-focus {i + 1 ===
					start
						? 'outline outline-2 outline-selected'
						: ''} {owners[i] === -1 ? 'bg-neutral-400/30' : ''}"
					style="left: {b.left}%; top: {b.top}%; width: {b.width}%; height: {b.height}%"
					title={owners[i] === -1
						? `Label ${i + 1}: used`
						: owners[i] === null
							? `Label ${i + 1}: start here`
							: `Label ${i + 1}: ${items[owners[i]!]?.name}`}
					aria-label="Start at label {i + 1}"
					onclick={() => (chosenStart = i + 1)}
				>
					{#if owners[i] !== null && owners[i]! >= 0}
						<span
							class="absolute bottom-1 left-1 inline-flex h-4 min-w-4 items-center justify-center rounded-sm bg-selected px-0.5 text-[10px] font-semibold text-selected-foreground"
							>{owners[i]! + 1}</span
						>
					{/if}
				</button>
			{/each}
		</div>
		<p class="mt-2 text-center text-xs text-muted-foreground">
			The first sheet as it prints{pages > 1 ? `; ${pages - 1} more after it` : ''}. Grey: used
			labels. The numbers are the roasts on the left.
		</p>
	</section>
</div>
