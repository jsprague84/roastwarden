<script lang="ts">
	/** One bean: details, stock with its ledger, and the roasts that used it (docs/batches.md). */
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import { goto, refreshAll } from '$app/navigation';
	import {
		beans as beansApi,
		type BeanDetail,
		type BeanRequest,
		type StockMove
	} from '#lib/api/beans.js';
	import {
		draftFromMove,
		emptyPurchase,
		fmtMoney,
		fmtPerKg,
		localDate,
		purchaseBody,
		type PurchaseDraft
	} from '#lib/beans/money.js';
	import PurchaseForm from '#lib/components/beans/PurchaseForm.svelte';
	import { notifications } from '#lib/stores/notifications.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';
	import { weightUnit } from '#lib/stores/units.svelte.js';
	import { batchLabel, fmtDelta, fmtGrams } from '#lib/beans/stock.js';
	import BeanForm from '#lib/components/beans/BeanForm.svelte';
	import BeanGuidePanel from '#lib/components/beans/BeanGuidePanel.svelte';
	import { ArrowLeft } from '@lucide/svelte';
	import RangeBand from '#lib/components/ui/RangeBand.svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { judge } from '#lib/guidance/judge.js';
	import {
		densityClass,
		greenMoistureRange,
		refBasis,
		sourceById
	} from '#lib/guidance/references.js';
	import { isOn } from '#lib/stores/modules.svelte.js';

	let { data }: PageProps = $props();
	const bean = $derived<BeanDetail>(data.bean);

	let editing = $state(false);
	let busy = $state(false);
	// svelte-ignore state_referenced_locally
	let purchase = $state<PurchaseDraft>(emptyPurchase(data.bean.supplier ?? ''));
	let setTo = $state<number | null>(null);
	let note = $state('');
	let editingMove = $state<string | null>(null);
	let moveDraft = $state<PurchaseDraft>(emptyPurchase());
	/** The home currency: the bean's, else the last purchase's, else USD. */
	const home = $derived(bean.currency ?? bean.moves.find((m) => m.currency)?.currency ?? 'USD');
	const suppliers = $derived([
		...new Set(
			[bean.supplier, ...bean.moves.map((m) => m.supplier)].filter((s): s is string => !!s)
		)
	]);

	async function act(what: string, fn: () => Promise<unknown>): Promise<boolean> {
		busy = true;
		try {
			await fn();
			await refreshAll();
			return true;
		} catch (e) {
			notifications.add(`${what}: ${e instanceof Error ? e.message : e}`, 'error');
			return false;
		} finally {
			busy = false;
		}
	}

	async function save(req: BeanRequest) {
		if (await act('Could not save', () => beansApi.update(bean.id, req))) editing = false;
	}

	async function addPurchase() {
		if (
			await act('Could not add the purchase', () => beansApi.stock(bean.id, purchaseBody(purchase)))
		) {
			purchase = emptyPurchase(purchase.supplier);
		}
	}

	function editMove(m: StockMove) {
		editingMove = m.id;
		moveDraft = draftFromMove(m);
	}

	async function saveMove(m: StockMove) {
		const body = purchaseBody(moveDraft, localDate(m.at));
		// A correction has no price fields: send only what it has.
		const req =
			m.kind === 'purchase'
				? body
				: { delta_g: body.delta_g, note: body.note, ...('at' in body ? { at: body.at } : {}) };
		if (await act('Could not save the move', () => beansApi.updateMove(bean.id, m.id, req)))
			editingMove = null;
	}

	async function deleteMove(m: StockMove) {
		if (
			!confirm(
				`Remove this ${m.kind === 'purchase' ? 'purchase' : 'correction'} of ${fmtDelta(m.delta_g)} from the ledger?`
			)
		)
			return;
		if (await act('Could not remove it', () => beansApi.deleteMove(bean.id, m.id)))
			editingMove = null;
	}

	async function setStock(e: SubmitEvent) {
		e.preventDefault();
		const g = setTo;
		if (g == null) return;
		if (
			await act('Could not set the stock', () =>
				beansApi.stock(bean.id, { set_g: g, note: note.trim() || 'counted' })
			)
		) {
			setTo = null;
			note = '';
		}
	}

	async function archive(on: boolean) {
		await act('Could not change it', () => beansApi.update(bean.id, { archived: on }));
	}

	async function remove() {
		if (!confirm(`Delete ${bean.name}? Its stock history goes too.`)) return;
		busy = true;
		try {
			await beansApi.delete(bean.id);
			goto(resolve('/beans'));
		} catch (e) {
			notifications.add(`Could not delete: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			busy = false;
		}
	}

	const KIND: Record<string, string> = {
		purchase: 'Purchase',
		adjust: 'Correction',
		roast: 'Roast'
	};
	const fmtDate = (iso: string) =>
		new Date(iso).toLocaleString(undefined, {
			month: 'short',
			day: 'numeric',
			year: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	const roastFor = (sid: string | null) =>
		sid ? bean.roasts.find((r) => r.id === sid) : undefined;
	const facts = $derived(
		[
			['Origin', [bean.origin, bean.region].filter(Boolean).join(' · ') || null],
			['Producer', bean.producer],
			['Variety', bean.variety],
			['Process', [bean.process, bean.drying].filter(Boolean).join(' · ') || null],
			['Altitude', bean.altitude],
			[
				'Grade',
				[bean.grade, bean.screen_size && `screen ${bean.screen_size}`]
					.filter(Boolean)
					.join(' · ') || null
			],
			['Lot', bean.lot],
			['Supplier', bean.supplier],
			['Score', bean.score != null ? String(bean.score) : null],
			['Certifications', bean.certifications],
			['Roast', bean.roast_recommendation],
			['SKU', bean.sku],
			['Moisture', bean.moisture_pct != null ? `${bean.moisture_pct} %` : null],
			['Density', bean.density_g_l != null ? `${bean.density_g_l} g/L` : null],
			['Low below', fmtGrams(bean.low_stock_g)]
		].filter((f): f is [string, string] => !!f[1])
	);

	// Roast guidance (docs/roast-guidance.md): moisture against ICO 420, density by one importer's classes.
	const moistureJ = $derived(judge(bean.moisture_pct, greenMoistureRange()));
	const kornman = sourceById('kornman2016')!;

	const field =
		'w-28 rounded-md border border-border bg-input px-2.5 py-1.5 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
	const th = 'px-3 py-2 text-left text-xs font-medium text-muted-foreground';
</script>

<svelte:head>
	<title>{bean.name} | Roastwarden</title>
</svelte:head>

<div class="space-y-6">
	<div>
		<a
			href={resolve('/beans')}
			class="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
			><ArrowLeft class="h-3.5 w-3.5" /> Beans</a
		>
		<div class="mt-1 flex flex-wrap items-end justify-between gap-3">
			<div>
				<h1 class="text-2xl font-bold text-foreground">{bean.name}</h1>
				{#if bean.archived}<p class="text-sm text-muted-foreground">
						Archived: hidden from the pickers.
					</p>{/if}
			</div>
			<div class="flex gap-2">
				{#if isOn('reports')}<a
						href={resolve(`/reports?kind=coffee&bean=${encodeURIComponent(bean.id)}`)}
						class={secondary}
						title="This coffee's report: its roasts, cuppings and ledger as a PDF">Report</a
					>{/if}
				<button class={secondary} onclick={() => (editing = !editing)}
					>{editing ? 'Close' : 'Edit'}</button
				>
				{#if bean.roast_count > 0}
					<button class={secondary} onclick={() => archive(!bean.archived)} disabled={busy}
						>{bean.archived ? 'Unarchive' : 'Archive'}</button
					>
				{:else}
					<button class={secondary} onclick={remove} disabled={busy}>Delete</button>
				{/if}
			</div>
		</div>
	</div>

	{#if editing}
		<div class="rounded-md border border-border bg-card p-3">
			{#key bean.updated_at}
				<BeanForm {bean} {busy} onsave={save} oncancel={() => (editing = false)} />
			{/key}
		</div>
	{:else if facts.length || bean.notes || bean.url || bean.cupping_notes || bean.label_notes}
		<dl class="grid grid-cols-2 gap-x-6 gap-y-1 text-sm sm:grid-cols-4">
			{#each facts as [k, v] (k)}
				<div>
					<dt class="text-xs text-muted-foreground">{k}</dt>
					<dd class="text-foreground">
						{#if k === 'Moisture'}
							<RangeBand j={moistureJ} range="8–12.5 %" level="ICO Resolution 420">{v}</RangeBand>
						{:else if k === 'Density' && bean.density_g_l != null}
							{v} <span class="text-muted-foreground">· {densityClass(bean.density_g_l)}</span>
							<Hint
								inline
								help={{
									line: 'Free-settled density: about 640 g/L is low, 670 average, above 690 dense.',
									more: refBasis('density_green')
								}}
								links={[{ title: kornman.title, url: kornman.url }]}
							/>
						{:else}{v}{/if}
					</dd>
				</div>
			{/each}
			{#if bean.label_notes || bean.short_name}<div class="col-span-2 sm:col-span-4">
					<dt class="text-xs text-muted-foreground">On its labels</dt>
					<dd class="text-foreground">
						{[bean.short_name, bean.label_notes].filter(Boolean).join(': ')}
					</dd>
				</div>{/if}
			{#if bean.cupping_notes}<div class="col-span-2 sm:col-span-4">
					<dt class="text-xs text-muted-foreground">Cupping notes</dt>
					<dd class="text-foreground">{bean.cupping_notes}</dd>
				</div>{/if}
			{#if bean.notes}<div class="col-span-2 sm:col-span-4">
					<dt class="text-xs text-muted-foreground">Notes</dt>
					<dd class="text-foreground">{bean.notes}</dd>
				</div>{/if}
			{#if bean.details && Object.keys(bean.details).length}
				<div class="col-span-2 sm:col-span-4">
					<dt class="text-xs text-muted-foreground">From the supplier</dt>
					<dd class="text-xs text-muted-foreground">
						{Object.entries(bean.details)
							.map(([k, v]) => `${k}: ${v}`)
							.join(' · ')}
					</dd>
				</div>
			{/if}
			{#if bean.url}<div class="col-span-2 sm:col-span-4">
					<a
						href={bean.url}
						target="_blank"
						rel="external noopener noreferrer"
						class="text-sm text-foreground underline underline-offset-2 [overflow-wrap:anywhere]"
						>The offer at {bean.supplier ?? new URL(bean.url).host}</a
					>
				</div>{/if}
		</dl>
	{/if}

	<!-- Before its first roast, what to expect; after one, what to change (docs/new-bean.md) -->
	{#key bean.updated_at}
		<BeanGuidePanel beanId={bean.id} pickProfile />
	{/key}

	<!-- Stock -->
	<section aria-labelledby="stock-h" class="space-y-3">
		<div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
			<h2 id="stock-h" class="text-lg font-semibold text-foreground">Stock</h2>
			<span
				class="text-2xl font-semibold tabular-nums {bean.low
					? 'text-amber-400'
					: 'text-foreground'}">{fmtGrams(bean.stock_g)}</span
			>
			{#if bean.low}<span class="text-xs text-amber-400"
					>{bean.stock_g <= 0 ? 'out of stock' : `low: below ${fmtGrams(bean.low_stock_g)}`}</span
				>{/if}
			{#if bean.cost_per_kg != null && isOn('costs')}
				<span class="text-sm tabular-nums text-foreground"
					>{fmtPerKg(bean.cost_per_kg, bean.currency)}</span
				>
				{#if bean.stock_value != null}<span class="text-xs text-muted-foreground"
						>stock worth {fmtMoney(bean.stock_value, bean.currency)}</span
					>{/if}
				<Hint
					inline
					help={{
						line: 'The weighted average of what is left: each priced purchase, with its shipping and fees, mixes into the stock held then; roasts and corrections take beans out at that average.'
					}}
				/>
			{/if}
		</div>
		<div class="space-y-3">
			<div>
				<h3 class="mb-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">
					Purchase
				</h3>
				<PurchaseForm
					bind:draft={purchase}
					{busy}
					currency={home}
					{suppliers}
					onsubmit={addPurchase}
				/>
			</div>
			<form onsubmit={setStock} class="flex flex-wrap items-end gap-2">
				<label class="text-xs text-muted-foreground"
					>Counted ({weightUnit()})<WeightInput
						bind:grams={setTo}
						class="{field} mt-0.5 block"
					/></label
				>
				<label class="text-xs text-muted-foreground"
					>Note<input
						bind:value={note}
						placeholder="counted"
						class="{field} mt-0.5 block w-48"
					/></label
				>
				<button
					type="submit"
					class={secondary}
					disabled={busy || setTo == null}
					title="Writes the difference to the current stock as a correction">Set stock</button
				>
			</form>
		</div>

		{#if bean.moves.length}
			<div class="overflow-x-auto rounded-md border border-border bg-card">
				<table class="w-full text-sm">
					<thead class="border-b border-border">
						<tr
							><th class={th}>When</th><th class={th}>What</th><th class="{th} text-right"
								>Change</th
							>{#if isOn('costs')}<th class="{th} text-right">Cost</th>{/if}<th class={th}
								><span class="sr-only">Edit</span></th
							></tr
						>
					</thead>
					<tbody class="divide-y divide-border">
						{#each bean.moves as m (m.id)}
							{@const r = roastFor(m.session_id)}
							<tr>
								<td class="px-3 py-1.5 text-xs text-muted-foreground">{fmtDate(m.at)}</td>
								<td class="px-3 py-1.5">
									{KIND[m.kind] ?? m.kind}
									{#if r}<a
											href={resolve('/history/[id]', { id: r.id })}
											class="ml-1 text-xs text-muted-foreground hover:text-foreground hover:underline"
											>{batchLabel(r.batch_number)} {r.name}</a
										>{/if}
									{#if m.supplier}<span class="ml-1 text-xs text-muted-foreground"
											>{m.supplier}</span
										>{/if}
									{#if m.crop_year || m.arrival}<span class="ml-1 text-xs text-muted-foreground"
											>{[m.crop_year && `crop ${m.crop_year}`, m.arrival && `arrived ${m.arrival}`]
												.filter(Boolean)
												.join(', ')}</span
										>{/if}
									{#if m.note}<span class="ml-1 text-xs text-muted-foreground">{m.note}</span>{/if}
									{#if isOn('costs') && m.price != null && m.currency && m.currency !== home}<span
											class="ml-1 text-xs text-amber-400"
											title="Not in the average: another currency"
											>{fmtMoney(m.price, m.currency)}</span
										>{/if}
								</td>
								<td
									class="px-3 py-1.5 text-right tabular-nums {m.delta_g < 0
										? 'text-muted-foreground'
										: 'text-foreground'}">{fmtDelta(m.delta_g)}</td
								>
								{#if isOn('costs')}<td
										class="px-3 py-1.5 text-right text-xs tabular-nums {m.kind === 'purchase'
											? 'text-foreground'
											: 'text-muted-foreground'}"
										title={m.cost_per_kg != null
											? `${fmtPerKg(m.cost_per_kg, home)} after this`
											: undefined}>{m.cost != null ? fmtMoney(m.cost, home) : ''}</td
									>{/if}
								<td class="px-2 py-1.5 text-right">
									{#if m.kind !== 'roast'}<button
											class="text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
											onclick={() => (editingMove === m.id ? (editingMove = null) : editMove(m))}
											>{editingMove === m.id ? 'Close' : 'Edit'}</button
										>{/if}
								</td>
							</tr>
							{#if editingMove === m.id}
								<tr>
									<td colspan={isOn('costs') ? 5 : 4} class="bg-background/40 px-3 py-2">
										<PurchaseForm
											bind:draft={moveDraft}
											{busy}
											currency={home}
											{suppliers}
											correction={m.kind !== 'purchase'}
											submitLabel="Save"
											onsubmit={() => saveMove(m)}
											oncancel={() => (editingMove = null)}
											ondelete={() => deleteMove(m)}
										/>
									</td>
								</tr>
							{/if}
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	</section>

	{#if bean.roasts.length}
		<section aria-labelledby="roasts-h" class="space-y-2">
			<h2 id="roasts-h" class="text-lg font-semibold text-foreground">
				Roasts <span class="text-sm font-normal text-muted-foreground">{bean.roasts.length}</span>
			</h2>
			<ul class="divide-y divide-border rounded-md border border-border bg-card text-sm">
				{#each bean.roasts as r (r.id)}
					<li>
						<a
							href={resolve('/history/[id]', { id: r.id })}
							class="flex items-baseline gap-3 px-3 py-2 hover:bg-accent"
						>
							<span class="w-10 shrink-0 tabular-nums text-muted-foreground"
								>{batchLabel(r.batch_number)}</span
							>
							<span class="min-w-0 flex-1 truncate text-foreground">{r.name}</span>
							<span class="text-xs tabular-nums text-muted-foreground"
								>{fmtGrams(r.green_weight)}{r.weight_loss_pct != null
									? ` · ${r.weight_loss_pct.toFixed(1)} % loss`
									: ''}</span
							>
							<span class="hidden text-xs text-muted-foreground sm:inline"
								>{fmtDate(r.start_time ?? r.created_at)}</span
							>
						</a>
					</li>
				{/each}
			</ul>
		</section>
	{/if}
</div>
