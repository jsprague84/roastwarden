<script lang="ts">
	/** Bag labels for a roast (docs/labels.md): pick the stock, design and first label, see
	 *  the sheet as it will print, download or print it, mark it printed. The item's fields are
	 *  the same editor as each roast on Label Studio → Print; the actions stay in view.
	 *  Calibration is folded below. */
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import { labelStudio } from '#lib/api/labelStudio.js';
	import { labels as labelsApi } from '#lib/api/labels.js';
	import { roasted as roastedApi } from '#lib/api/roasted.js';
	import { settings as settingsApi } from '#lib/api/settings.js';
	import { slotBoxes, sheetsNeeded, type LabelOptions } from '#lib/labels/labels.js';
	import { newItem, packOffer, runBody, type RunItemAsk } from '#lib/labels/runs.js';
	import { fmtGrams } from '#lib/beans/stock.js';
	import { bagDays, blendLine } from '#lib/beans/roasted.js';
	import { notifications } from '#lib/stores/notifications.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import LabelItemFields from '#lib/components/labels/LabelItemFields.svelte';
	import RunActions from '#lib/components/labels/RunActions.svelte';
	import PrinterPick from '#lib/components/labels/PrinterPick.svelte';
	import type { LabelPrinter } from '#lib/api/labelStudio.js';
	import { untrack } from 'svelte';
	import { isOn } from '#lib/stores/modules.svelte.js';

	let { data }: PageProps = $props();
	const s = $derived(data.session);
	// The choices as loaded; reloaded after a print or an offset is saved
	let options = $state<LabelOptions>(untrack(() => data.options));

	// The stock, design and fields are this browser's choice from last time
	const STORE = 'roastwarden.labels';
	function remembered(): { sheet?: string; design?: string; hide?: string[]; qr?: string } {
		try {
			return JSON.parse(localStorage.getItem(STORE) ?? '{}');
		} catch {
			return {};
		}
	}
	const last = remembered();
	// The coffee's own label (a bean's design, settings and brand) comes first: no design
	// named is the coffee's own
	const own = untrack(() => data.bean);
	let sheetId = $state(
		untrack(() =>
			options.sheets.some((x) => x.id === last.sheet)
				? last.sheet!
				: (options.sheets[0]?.id ?? '5163')
		)
	);
	let item = $state<RunItemAsk>(
		untrack(() =>
			newItem(data.session.id, data.session.name, {
				design: own?.label_design
					? ''
					: options.designs.some((d) => d.id === last.design && d.id !== 'plain')
						? last.design!
						: '',
				hide: last.hide ?? []
			})
		)
	);
	// What the QR code opens: this browser's last choice, else the server's (`label_qr`)
	let qr = $state(
		['app', 'share', 'supplier', 'none'].includes(last.qr ?? '')
			? last.qr!
			: (untrack(() => options.qr) ?? 'app')
	);

	const sheet = $derived(options.sheets.find((x) => x.id === sheetId) ?? options.sheets[0]);
	const per = $derived(sheet.cols * sheet.rows);
	// The sheet's next free label (where the last print left it), until a label is clicked
	let start = $derived(sheet.next_label ?? 1);
	const pages = $derived(sheetsNeeded(sheet, item.count, start));

	$effect(() => {
		try {
			localStorage.setItem(
				STORE,
				JSON.stringify({ sheet: sheetId, design: item.design, hide: item.hide, qr })
			);
		} catch {
			/* blocked storage: it just forgets */
		}
	});

	// The printer (its calibration lays the sheet out), tray and colour for a print
	let printerId = $state('');
	let tray = $state('');
	let color = $state('');
	let printerList = $state<LabelPrinter[]>([]);

	// The run: one roast's labels, the same body for the preview, Download and Print
	const body = $derived(
		runBody([item], { sheet: sheetId, start: Math.min(start, per), printer: printerId, qr })
	);

	// Take the bags off the roast (docs/roasted.md, "Packed bags"): off unless the owner ticks it.
	// Labelling your own bags right after roasting is storage, not packing: the coffee is still yours
	let lot = $state(untrack(() => data.lot));
	// A roast in one bag with others: the label and the bags are the bag's (docs/roasted.md,
	// "Roasts in one bag")
	const bag = $derived(lot?.combined ?? null);
	const offer = $derived(
		packOffer(
			item.count,
			item.weight_g,
			bag?.roasted_g ?? s.roasted_weight,
			bag?.left_g ?? lot?.left_g,
			bag ? `the bag #${bag.batches}` : 'this roast'
		)
	);
	let packChoice = $state<boolean | null>(null);
	const pack = $derived(!!offer && (packChoice ?? false));
	const runAsk = $derived(
		runBody([{ ...item, pack }], {
			sheet: sheetId,
			start: Math.min(start, per),
			printer: printerId,
			qr
		})
	);
	async function reloadLot() {
		lot = await roastedApi.lot(s.id).catch(() => lot);
		packChoice = null;
	}

	// The preview: the first page as the server lays it out, a moment after the last change
	let svg = $state('');
	let problem = $state('');
	let loading = $state(false);
	$effect(() => {
		const b = body;
		// A newer change aborts the older answer, so the last one shown is the last asked
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
		}, 250);
		return () => {
			clearTimeout(t);
			controller.abort();
		};
	});

	async function reloadSheets() {
		options = await labelsApi.options();
	}

	// Keep this design, its settings and brand as the coffee's own label
	let keeping = $state(false);
	async function keepForBean() {
		if (!own) return;
		keeping = true;
		const design = item.design || own.label_design || 'plain';
		// No design named: the coffee's own settings under the changes
		const params = item.design
			? (item.params ?? {})
			: { ...(own.label_params ?? {}), ...(item.params ?? {}) };
		try {
			await labelStudio.setBeanLabel(own.id, {
				design,
				params,
				brand: item.brand || own.label_brand || null
			});
			notifications.add(`${own.name}: its labels use ${design}`, 'success');
		} catch (e) {
			notifications.add(`Labels: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			keeping = false;
		}
	}

	// Calibration: the sheet's offset, saved for every browser and for `roast labels`
	// The chosen printer's calibration for this sheet, else the sheet's own offset
	let dx = $state(0);
	let dy = $state(0);
	const chosenPrinter = $derived(printerList.find((p) => p.id === printerId));
	$effect(() => {
		const c = chosenPrinter?.calibrations.find((x) => x.sheet === sheetId);
		dx = c?.x_mm ?? sheet.offset.x_mm;
		dy = c?.y_mm ?? sheet.offset.y_mm;
	});
	let saving = $state(false);
	async function saveOffset() {
		saving = true;
		try {
			if (chosenPrinter) {
				await labelStudio.calibrate(chosenPrinter.id, sheetId, {
					x_mm: Number(dx) || 0,
					y_mm: Number(dy) || 0
				});
				printerList = await labelStudio.printers();
				notifications.add(`${chosenPrinter.name}: offset saved for ${sheet.name}`, 'success');
				return;
			}
			const all: Record<string, { x_mm: number; y_mm: number }> = {};
			for (const x of options.sheets) if (x.offset.x_mm || x.offset.y_mm) all[x.id] = x.offset;
			all[sheetId] = { x_mm: Number(dx) || 0, y_mm: Number(dy) || 0 };
			await settingsApi.set('label_offsets', JSON.stringify(all));
			options = await labelsApi.options();
			notifications.add(`Offset saved for ${sheet.name}`, 'success');
		} catch (e) {
			notifications.add(`Labels: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			saving = false;
		}
	}

	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const lbl = 'block text-xs font-medium text-muted-foreground';
	const used = $derived(
		start > 1 ? (start === 2 ? 'label 1 used' : `labels 1–${start - 1} used`) : 'a new sheet'
	);
</script>

<svelte:head>
	<title>Labels · {s.name}</title>
</svelte:head>

<div class="mb-3 flex flex-wrap items-center gap-x-4 gap-y-2">
	<a
		href={resolve('/history/[id]', { id: s.id })}
		class="text-sm text-muted-foreground hover:text-foreground">&larr; Roast</a
	>
	<h1 class="text-xl font-bold text-foreground">
		Labels · {#if s.batch_number}<span class="tabular-nums text-muted-foreground"
				>#{s.batch_number}</span
			>{/if}
		{s.name}
	</h1>
	{#if isOn('reports')}<a
			href={resolve(`/reports?kind=roast&roast=${encodeURIComponent(s.id)}`)}
			class="text-sm text-action hover:underline">Its report</a
		>{/if}
</div>

<div class="grid gap-4 pb-24 lg:grid-cols-[20rem_minmax(0,1fr)] lg:pb-0">
	<section class="space-y-3 rounded-md border border-border bg-card p-4">
		<label class={lbl}
			>Label sheet
			<select bind:value={sheetId} class={field}>
				{#each options.sheets as x (x.id)}<option value={x.id}>{x.name}</option>{/each}
			</select>
		</label>
		<label class={lbl}
			>Start at label
			<input type="number" min="1" max={per} bind:value={start} class={field} />
		</label>
		<p class="-mt-1 text-xs text-muted-foreground">
			Load {sheet.name.split(',')[0]}, {used}{start === (sheet.next_label ?? 1) && start > 1
				? ' (as the last print left it)'
				: ''}.
			{#if start > 1}<button class="text-action hover:underline" onclick={() => (start = 1)}
					>A new sheet</button
				>{/if}
			Click a label in the preview to start there. {pages === 1 ? 'One sheet.' : `${pages} sheets.`}
			<a class="text-action hover:underline" href={resolve(`/labels/print?roast=${s.id}`)}
				>More roasts on this sheet</a
			>
		</p>

		<LabelItemFields
			bind:item
			designs={options.designs}
			brands={data.brands}
			fields={options.fields}
			sheet={sheetId}
			{own}
			roastedWeight={bag?.roasted_g ?? s.roasted_weight}
		/>
		{#if bag}
			<p class="-mt-1 text-xs text-muted-foreground" data-testid="bag-label">
				{#if bag.blend}
					A blend, batches #{bag.batches} ({blendLine(bag)}): the label prints each coffee's origin
					with its share, every batch, roasted {bagDays(bag)}, best from when the newest is ready.
					Its tasting notes are yours to write (More → Tasting notes): one coffee's would not
					describe the blend.
				{:else}
					In one bag with batches #{bag.batches}: the label prints them all, roasted {bagDays(bag)},
					best from when the newest is ready.
				{/if}
			</p>
		{/if}
		{#if own}
			<button
				onclick={keepForBean}
				disabled={keeping}
				class="text-left text-xs text-action hover:underline disabled:opacity-50"
				title="Its roasts' labels start with this design, these settings and this brand"
			>
				{keeping ? 'Saving…' : `Use for ${own.name}`}
			</button>
		{/if}
		{#if offer && lot}
			<label
				class="flex items-center gap-2 text-sm text-foreground"
				title="Records the bags as packed from this roast when the labels are made: the grams come off what is left. Leave it off for your own coffee: a label on a bag you keep changes nothing"
			>
				<input
					type="checkbox"
					checked={pack}
					onchange={(e) => (packChoice = e.currentTarget.checked)}
					data-testid="pack-bags"
				/>
				{offer.text}, sold or given
			</label>
			{#if !offer.enough}
				<p class="-mt-2 text-xs text-muted-foreground">
					Only {fmtGrams(bag?.left_g ?? lot.left_g)} of it is left.
				</p>
			{/if}
		{/if}
		<Hint
			help={{
				line: `Best from: the roast date plus a rest, ${options.rest_days.espresso_light} days for a light espresso roast, ${options.rest_days.filter_light} for a light filter roast.`,
				more: `Darker roasts: ${options.rest_days.espresso} days for espresso, ${options.rest_days.filter} for filter. Change them in Settings → Roasting.`
			}}
		/>
		<label class={lbl}
			>QR code opens
			<select bind:value={qr} class={field} data-testid="qr-choice">
				<option value="app">This roast in Roastwarden (your login)</option>
				<option value="share">Its share page (anyone with the bag)</option>
				<option value="supplier">The coffee at its supplier</option>
				<option value="none">No QR code</option>
			</select>
		</label>
		{#if qr === 'share'}
			<p class="-mt-2 text-xs text-muted-foreground">
				A public page with the coffee, roast date, notes and curve; never its cost.
				<a class="text-action hover:underline" href={resolve('/history/[id]#share', { id: s.id })}
					>The share page</a
				>
			</p>
		{/if}
		<PrinterPick bind:printer={printerId} bind:tray bind:color bind:printers={printerList} />

		<details class="border-t border-border pt-3">
			<summary class="cursor-pointer text-sm font-medium text-foreground"
				>Calibrate this sheet{chosenPrinter ? ` on ${chosenPrinter.name}` : ''}</summary
			>
			<p class="mt-2 text-xs text-muted-foreground">
				Print the calibration page on plain paper, hold it over a sheet of labels against the light,
				and move the marks onto the label corners: right and down are positive.
			</p>
			<button
				onclick={() =>
					labelStudio
						.calibrationPdf(sheetId, printerId || undefined)
						.catch((e) => notifications.add(`Labels: ${e}`, 'error'))}
				class="mt-2 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
			>
				Calibration page (PDF)
			</button>
			<div class="mt-2 grid grid-cols-2 gap-3">
				<label class={lbl}
					>Right, mm
					<input type="number" step="0.1" min="-20" max="20" bind:value={dx} class={field} />
				</label>
				<label class={lbl}
					>Down, mm
					<input type="number" step="0.1" min="-20" max="20" bind:value={dy} class={field} />
				</label>
			</div>
			<button
				onclick={saveOffset}
				disabled={saving}
				class="mt-2 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-50"
			>
				{saving ? 'Saving…' : 'Save offset'}
			</button>
		</details>
	</section>

	<section class="min-w-0">
		<RunActions
			ask={() => runAsk}
			disabled={!!problem}
			why={problem}
			{per}
			{printerId}
			{tray}
			{color}
			loadHint={`Load a ${sheet.name} sheet${start > 1 ? `, labels 1–${start - 1} used,` : ''} then print`}
			onsheets={reloadSheets}
			onmade={reloadLot}
		/>
		{#if problem}
			<pre
				class="mb-3 whitespace-pre-wrap rounded-md border border-border bg-card p-3 text-sm text-foreground">{problem}</pre>
		{/if}
		<div
			class="relative mx-auto w-full max-w-[44rem] bg-white shadow"
			style="aspect-ratio: {sheet.page_w} / {sheet.page_h}"
			aria-busy={loading}
		>
			{#if svg}
				<!-- eslint-disable-next-line svelte/no-at-html-tags -- the server's own label render (Typst → SVG) -->
				<div class="absolute inset-0 [&>svg]:h-full [&>svg]:w-full">{@html svg}</div>
			{/if}
			{#each slotBoxes(sheet) as b, i (i)}
				<button
					class="absolute rounded-[3%] hover:outline hover:outline-2 hover:outline-focus {i + 1 ===
					start
						? 'outline outline-2 outline-selected'
						: ''} {i + 1 < start ? 'bg-neutral-400/25' : ''}"
					style="left: {b.left}%; top: {b.top}%; width: {b.width}%; height: {b.height}%"
					title={i + 1 < start ? `Label ${i + 1}: left blank` : `Start at label ${i + 1}`}
					aria-label="Start at label {i + 1}"
					onclick={() => (start = i + 1)}
				></button>
			{/each}
		</div>
		<p class="mt-2 text-center text-xs text-muted-foreground">
			The first sheet, as it prints{pages > 1 ? `; ${pages - 1} more after it` : ''}. Grey: labels
			left blank.
		</p>
	</section>
</div>
