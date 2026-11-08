<script lang="ts">
	/** One supplier order (docs/green-coffee.md, "Orders"): its lines and beans, the totals check, shipped and received, its receipts. */
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import { goto, refreshAll } from '$app/navigation';
	import { downloadFile } from '#lib/api/http.js';
	import { orders, type PurchaseOrder, type Receipt } from '#lib/api/orders.js';
	import { fmtMoney, localDate } from '#lib/beans/money.js';
	import { fmtGrams } from '#lib/beans/stock.js';
	import {
		fileSize,
		isOpen,
		orderName,
		receivedLines,
		statusText,
		totals
	} from '#lib/beans/orders.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';
	import { weightUnit } from '#lib/stores/units.svelte.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { ArrowLeft, Download, Trash2 } from '@lucide/svelte';

	let { data }: PageProps = $props();
	const o = $derived<PurchaseOrder>(data.order);
	const t = $derived(totals(o));
	const cur = $derived(o.currency);

	let busy = $state(false);
	let carrier = $state('');
	let tracking = $state('');
	let shippedOn = $state(localDate(new Date().toISOString()));
	let receivedOn = $state(localDate(new Date().toISOString()));
	/** What each coffee line weighed when it came, by line id (blank: as ordered) */
	// svelte-ignore state_referenced_locally
	let arrived = $state<Record<string, number | null>>(
		Object.fromEntries(data.order.lines.map((l: { id: string }) => [l.id, null]))
	);
	let receiptKind = $state<Receipt['kind']>('confirmation');
	let fileInput: HTMLInputElement | undefined = $state();

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

	async function shipped(e: SubmitEvent) {
		e.preventDefault();
		await act('Could not record the shipment', () =>
			orders.shipped(o.id, {
				carrier: carrier.trim() || undefined,
				tracking: tracking.trim() || undefined,
				at: shippedOn || undefined
			})
		);
	}

	async function received(e: SubmitEvent) {
		e.preventDefault();
		if (
			await act('Could not receive the order', () =>
				orders.received(o.id, {
					at: receivedOn || undefined,
					lines: receivedLines(o.lines, arrived)
				})
			)
		) {
			notifications.add('Received: the coffee is in stock.', 'success');
		}
	}

	async function cancel() {
		if (!confirm(`Cancel ${orderName(o)}? Its shopping-list entries go back on the list.`)) return;
		await act('Could not cancel it', () => orders.cancel(o.id));
	}

	async function remove() {
		const extra =
			o.status === 'received' ? ' The purchases it recorded go too: stock and costs change.' : '';
		if (!confirm(`Delete ${orderName(o)} and its receipts?${extra}`)) return;
		if (await act('Could not delete it', () => orders.remove(o.id))) goto(resolve('/beans/orders'));
	}

	async function addReceipt() {
		const file = fileInput?.files?.[0];
		if (!file) return;
		if (
			(await act('Could not keep the receipt', () => orders.addReceipt(o.id, file, receiptKind))) &&
			fileInput
		)
			fileInput.value = '';
	}

	async function download(r: Receipt) {
		try {
			await downloadFile(orders.receiptPath(o.id, r.id), r.file_name ?? `receipt-${r.id}`);
		} catch (e) {
			notifications.add(`Could not download it: ${e instanceof Error ? e.message : e}`, 'error');
		}
	}

	async function removeReceipt(r: Receipt) {
		if (!confirm(`Remove the receipt ${r.file_name ?? r.id}?`)) return;
		await act('Could not remove it', () => orders.removeReceipt(o.id, r.id));
	}

	const day = (iso: string | null) =>
		iso
			? new Date(iso).toLocaleDateString(undefined, {
					day: 'numeric',
					month: 'short',
					year: 'numeric'
				})
			: '–';
	const field =
		'rounded-md border border-border bg-input px-2.5 py-1.5 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
	const primary =
		'rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50';
	const th = 'px-3 py-2 text-left text-xs font-medium text-muted-foreground';
</script>

<svelte:head>
	<title>{orderName(o)} | Roastwarden</title>
</svelte:head>

<div class="space-y-6">
	<div>
		<a
			href={resolve('/beans/orders')}
			class="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
			><ArrowLeft class="h-3.5 w-3.5" /> Orders</a
		>
		<div class="mt-1 flex flex-wrap items-end justify-between gap-3">
			<div>
				<h1 class="text-2xl font-bold text-foreground">{o.supplier} order {o.order_number}</h1>
				<p class="text-sm text-muted-foreground">
					Placed {day(o.placed_at)} ·
					<span class={isOpen(o) ? 'text-highlight' : ''}>{statusText(o.status)}</span>
					{#if o.shipped_at}
						· shipped {day(o.shipped_at)}{o.tracking
							? ` (${o.carrier ?? 'tracking'} ${o.tracking})`
							: ''}{/if}
					{#if o.received_at}
						· received {day(o.received_at)}{/if}
				</p>
			</div>
			<div class="flex gap-2">
				{#if isOpen(o)}<button class={secondary} onclick={cancel} disabled={busy}
						>Cancel order</button
					>{/if}
				<button class={secondary} onclick={remove} disabled={busy}>Delete</button>
			</div>
		</div>
	</div>

	<section aria-labelledby="lines-h" class="space-y-2">
		<h2 id="lines-h" class="text-lg font-semibold text-foreground">Lines</h2>
		<div class="overflow-x-auto rounded-md border border-border bg-card">
			<table class="w-full text-sm">
				<thead class="border-b border-border">
					<tr>
						<th class={th}>#</th>
						<th class={th}>As on the receipt</th>
						<th class="{th} text-right">Coffee</th>
						<th class="{th} text-right">Price</th>
						<th class={th}>Bean</th>
						{#if isOpen(o)}<th class={th}>Came as ({weightUnit()})</th>{/if}
					</tr>
				</thead>
				<tbody class="divide-y divide-border">
					{#each o.lines as l (l.id)}
						<tr>
							<td class="px-3 py-2 text-xs tabular-nums text-muted-foreground">{l.position}</td>
							<td class="px-3 py-2">
								<div class="text-foreground">{l.name}</div>
								<div class="text-xs text-muted-foreground">
									{[l.bag && `${l.bag} × ${l.quantity}`, l.sku].filter(Boolean).join(' · ')}
								</div>
							</td>
							<td class="px-3 py-2 text-right tabular-nums">
								{l.kind === 'coffee' ? fmtGrams(l.received_g ?? l.weight_g ?? 0) : '–'}
								{#if l.received_g != null && l.weight_g != null}<div
										class="text-[11px] text-muted-foreground"
									>
										ordered {fmtGrams(l.weight_g)}
									</div>{/if}
							</td>
							<td class="px-3 py-2 text-right tabular-nums">{fmtMoney(l.price, cur)}</td>
							<td class="px-3 py-2 text-xs">
								{#if l.kind === 'other'}<span class="text-muted-foreground">not coffee</span>
								{:else if l.bean_id}<a
										href={resolve('/beans/[id]', { id: l.bean_id })}
										class="text-foreground hover:underline">{l.bean_name ?? l.bean_id}</a
									>
								{:else}<span class="text-amber-400">no bean</span>{/if}
							</td>
							{#if isOpen(o)}
								<td class="px-3 py-2">
									{#if l.kind === 'coffee'}<WeightInput
											bind:grams={arrived[l.id]}
											placeholder="as ordered"
											class="{field} w-28"
											aria-label="What line {l.position} weighed"
										/>{/if}
								</td>
							{/if}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
		<dl class="grid max-w-md grid-cols-2 gap-x-6 gap-y-0.5 text-sm tabular-nums">
			<dt class="text-muted-foreground">Lines</dt>
			<dd class="text-right">{fmtMoney(t.lines, cur)}</dd>
			{#if o.discount}<dt class="text-muted-foreground">Discount</dt>
				<dd class="text-right">−{fmtMoney(o.discount, cur)}</dd>{/if}
			<dt class="text-muted-foreground">Shipping</dt>
			<dd class="text-right">{fmtMoney(o.shipping, cur)}</dd>
			<dt class="text-muted-foreground">Tax</dt>
			<dd class="text-right">{fmtMoney(o.tax, cur)}</dd>
			<dt class="font-medium text-foreground">Total</dt>
			<dd class="text-right font-medium">{fmtMoney(o.total, cur)}</dd>
		</dl>
		{#if !t.addsUp}
			<p class="text-xs text-amber-400">
				The lines come to {fmtMoney(t.computed, cur)}, {fmtMoney(Math.abs(t.difference), cur)}
				{t.difference > 0 ? 'less' : 'more'} than the total.
				{o.mismatch_reason ? `Accepted: ${o.mismatch_reason}` : ''}
			</p>
		{/if}
		<p class="text-xs text-muted-foreground">
			Shipping goes over the coffee by weight; discount and tax by price (tax on the lines it was
			charged on). A line that is not coffee costs no bean anything.
		</p>
	</section>

	{#if isOpen(o)}
		<section aria-labelledby="state-h" class="space-y-3">
			<h2 id="state-h" class="text-lg font-semibold text-foreground">Shipped and received</h2>
			<form class="flex flex-wrap items-end gap-2" onsubmit={shipped}>
				<label class="text-xs text-muted-foreground"
					>Carrier<input
						bind:value={carrier}
						placeholder={o.carrier ?? 'USPS'}
						class="{field} mt-0.5 block w-28"
					/></label
				>
				<label class="text-xs text-muted-foreground"
					>Tracking<input
						bind:value={tracking}
						placeholder={o.tracking ?? ''}
						class="{field} mt-0.5 block w-48"
					/></label
				>
				<label class="text-xs text-muted-foreground"
					>Shipped<input type="date" bind:value={shippedOn} class="{field} mt-0.5 block" /></label
				>
				<button type="submit" class={secondary} disabled={busy}
					>{o.status === 'shipped' ? 'Update shipment' : 'Shipped'}</button
				>
			</form>
			<form class="flex flex-wrap items-end gap-2" onsubmit={received}>
				<label class="text-xs text-muted-foreground"
					>Came on<input type="date" bind:value={receivedOn} class="{field} mt-0.5 block" /></label
				>
				<button type="submit" class={primary} disabled={busy}>Received</button>
				<span class="text-xs text-muted-foreground"
					>Adds each coffee to its bean's stock, priced with its share of shipping and tax. A line
					that weighed different: fill in "Came as" first.</span
				>
			</form>
		</section>
	{/if}

	<section aria-labelledby="rcpt-h" class="space-y-2">
		<h2 id="rcpt-h" class="text-lg font-semibold text-foreground">Receipts</h2>
		<p class="text-xs text-muted-foreground">
			Kept as received, in the database and its backups. Private: they hold your address, phone and
			card digits, so only you (and an inventory or admin token) can download them; never on a share
			page or report.
		</p>
		{#if o.receipts.length}
			<ul class="divide-y divide-border rounded-md border border-border bg-card text-sm">
				{#each o.receipts as r (r.id)}
					<li class="flex items-center gap-3 px-3 py-2">
						<div class="min-w-0 flex-1">
							<div class="truncate text-foreground">{r.file_name ?? r.id}</div>
							<div class="text-xs text-muted-foreground">
								{r.kind} · {r.content_type} · {fileSize(r.size_bytes)} · added {day(r.added_at)}
							</div>
						</div>
						<button class={secondary} onclick={() => download(r)} title="Download"
							><Download class="h-4 w-4" /></button
						>
						<button
							class={secondary}
							onclick={() => removeReceipt(r)}
							disabled={busy}
							title="Remove"
							aria-label="Remove"><Trash2 class="h-4 w-4" /></button
						>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="text-sm text-muted-foreground">None kept.</p>
		{/if}
		<div class="flex flex-wrap items-end gap-2">
			<label class="text-xs text-muted-foreground"
				>Add a receipt<input
					bind:this={fileInput}
					type="file"
					accept=".eml,.html,.htm,.pdf,.png,.jpg,.jpeg,.webp,.heic,.txt"
					class="mt-0.5 block text-sm text-foreground"
				/></label
			>
			<label class="text-xs text-muted-foreground"
				>It is the
				<select bind:value={receiptKind} class="{field} mt-0.5 block">
					<option value="confirmation">confirmation</option>
					<option value="shipping">shipping notice</option>
					<option value="invoice">invoice</option>
					<option value="other">other</option>
				</select>
			</label>
			<button class={secondary} onclick={addReceipt} disabled={busy}>Keep it</button>
		</div>
	</section>
</div>
