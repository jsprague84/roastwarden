<script lang="ts">
	/** Supplier orders recorded from receipts (docs/green-coffee.md, "Orders"): open ones first, then the rest. */
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import type { PurchaseOrder } from '#lib/api/orders.js';
	import { fmtMoney } from '#lib/beans/money.js';
	import { fmtGrams } from '#lib/beans/stock.js';
	import { isOpen, lineSummary, orderName, statusText } from '#lib/beans/orders.js';
	import { ArrowLeft } from '@lucide/svelte';

	let { data }: PageProps = $props();
	const all = $derived<PurchaseOrder[]>(data.orders);
	const open = $derived(all.filter(isOpen));
	const done = $derived(all.filter((o) => !isOpen(o)));
	const day = (iso: string | null) =>
		iso
			? new Date(iso).toLocaleDateString(undefined, {
					day: 'numeric',
					month: 'short',
					year: 'numeric'
				})
			: '–';
	const th = 'px-3 py-2 text-left text-xs font-medium text-muted-foreground';
</script>

<svelte:head>
	<title>Orders | Roastwarden</title>
</svelte:head>

{#snippet table(list: PurchaseOrder[])}
	<div class="overflow-x-auto rounded-md border border-border bg-card">
		<table class="w-full text-sm">
			<thead class="border-b border-border">
				<tr>
					<th class={th}>Order</th>
					<th class={th}>Placed</th>
					<th class="{th} hidden sm:table-cell">Lines</th>
					<th class="{th} text-right">Coffee</th>
					<th class="{th} text-right">Total</th>
					<th class={th}>Status</th>
				</tr>
			</thead>
			<tbody class="divide-y divide-border">
				{#each list as o (o.id)}
					<tr class="hover:bg-accent">
						<td class="px-3 py-2"
							><a
								href={resolve('/beans/orders/[id]', { id: o.id })}
								class="font-medium text-foreground hover:underline">{orderName(o)}</a
							></td
						>
						<td class="px-3 py-2 text-xs text-muted-foreground">{day(o.placed_at)}</td>
						<td class="hidden px-3 py-2 text-xs text-muted-foreground sm:table-cell"
							>{lineSummary(o.lines)}</td
						>
						<td class="px-3 py-2 text-right tabular-nums">{fmtGrams(o.coffee_g)}</td>
						<td class="px-3 py-2 text-right tabular-nums">{fmtMoney(o.total, o.currency)}</td>
						<td class="px-3 py-2 text-xs {isOpen(o) ? 'text-highlight' : 'text-muted-foreground'}">
							{statusText(o.status)}{o.status === 'shipped' && o.tracking
								? ` · ${o.carrier ?? ''} ${o.tracking}`
								: ''}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/snippet}

<div class="space-y-6">
	<div>
		<a
			href={resolve('/beans')}
			class="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
			><ArrowLeft class="h-3.5 w-3.5" /> Beans</a
		>
		<h1 class="mt-1 text-2xl font-bold text-foreground">Orders</h1>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			Supplier orders recorded from their receipts. An agent reads the confirmation email or PDF (<code
				>roast order import</code
			>), the server checks the totals and matches each line to a bean. An order is on order until
			"Received": only then is it stock, with its share of shipping and tax.
		</p>
	</div>

	<section aria-labelledby="open-h" class="space-y-2">
		<h2 id="open-h" class="text-lg font-semibold text-foreground">On order</h2>
		{#if open.length}
			{@render table(open)}
		{:else}
			<p
				class="rounded-md border border-dashed border-border px-3 py-4 text-sm text-muted-foreground"
			>
				Nothing on order.
			</p>
		{/if}
	</section>

	{#if done.length}
		<section aria-labelledby="done-h" class="space-y-2">
			<h2 id="done-h" class="text-lg font-semibold text-foreground">Received and cancelled</h2>
			{@render table(done)}
		</section>
	{/if}
</div>
