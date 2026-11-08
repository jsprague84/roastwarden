<script lang="ts">
	/**
	 * The shopping list (docs/green-coffee.md, "Shopping list"): coffees wanted. "Bought"
	 * makes the purchase: the bean (new from the link's details, or the one named) and a
	 * stock move with the price seen, plus shipping.
	 */
	import { wanted as wantedApi, type Wanted } from '#lib/api/wanted.js';
	import { resolve } from '$app/paths';
	import { fmtGrams } from '#lib/beans/stock.js';
	import { fmtMoney, moneyOrNull } from '#lib/beans/money.js';
	import { weightUnit } from '#lib/stores/units.svelte.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';

	interface Props {
		items: Wanted[];
		currency?: string;
		onchange: (message?: string) => void;
		onerror: (message: string) => void;
	}
	let { items, currency = 'USD', onchange, onerror }: Props = $props();

	let open = $state<string | null>(null);
	let weight = $state<number | null>(null);
	let price = $state<number | string | null>('');
	let shipping = $state<number | string | null>('');
	let date = $state('');
	let busy = $state(false);

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	function start(w: Wanted) {
		open = open === w.id ? null : w.id;
		const bags = Math.max(1, w.quantity ?? 1);
		weight = w.weight_g != null ? w.weight_g * bags : null;
		price = w.price != null ? Math.round(w.price * bags * 100) / 100 : '';
		shipping = '';
		date = '';
	}

	async function bought(w: Wanted, e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		try {
			const r = await wantedApi.bought(w.id, {
				weight_g: weight ?? undefined,
				price: moneyOrNull(price) ?? undefined,
				shipping: moneyOrNull(shipping) ?? undefined,
				at: date || undefined
			});
			open = null;
			onchange(`Bought ${r.bean.name}: ${fmtGrams(r.bean.stock_g)} in stock`);
		} catch (err) {
			onerror(`Could not record it: ${msg(err)}`);
		} finally {
			busy = false;
		}
	}

	async function setQuantity(w: Wanted, q: number) {
		if (!Number.isInteger(q) || q < 1 || q > 100 || q === w.quantity) return;
		try {
			await wantedApi.update(w.id, { quantity: q });
			onchange();
		} catch (err) {
			onerror(`Could not change it: ${msg(err)}`);
		}
	}

	async function remove(w: Wanted) {
		if (!confirm(`Take ${w.name} off the shopping list?`)) return;
		try {
			await wantedApi.delete(w.id);
			onchange();
		} catch (err) {
			onerror(`Could not remove it: ${msg(err)}`);
		}
	}

	const field =
		'rounded-md border border-border bg-input px-2.5 py-1.5 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
	const link = 'text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground';
</script>

<ul class="divide-y divide-border rounded-md border border-border bg-card">
	{#each items as w (w.id)}
		<li class="px-3 py-2">
			<div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
				<span class="font-medium text-foreground">{w.name}</span>
				{#if w.status !== 'ordered'}
					<label class="text-xs text-muted-foreground" title="Bags of this size"
						>×
						<input
							type="number"
							min="1"
							max="100"
							value={w.quantity ?? 1}
							onchange={(e) => setQuantity(w, Number(e.currentTarget.value))}
							class="w-12 rounded border border-border bg-input px-1 py-0.5 text-xs text-foreground"
							aria-label="Bags of {w.name}"
						/>
					</label>
				{:else if (w.quantity ?? 1) > 1}<span class="text-xs text-muted-foreground"
						>× {w.quantity}</span
					>{/if}
				<span class="text-xs text-muted-foreground">
					{[
						w.supplier,
						w.size_label,
						w.price != null ? fmtMoney(w.price, w.currency ?? currency) : null,
						w.bean_name ? `again for ${w.bean_name}` : null
					]
						.filter(Boolean)
						.join(' · ')}
				</span>
				{#if w.notes}<span class="text-xs text-muted-foreground">{w.notes}</span>{/if}
				<span class="ml-auto flex gap-3">
					{#if w.url}<a href={w.url} target="_blank" rel="external noopener noreferrer" class={link}
							>Offer</a
						>{/if}
					{#if w.status === 'ordered'}
						<a
							href={resolve('/beans/orders')}
							class="text-xs text-highlight underline-offset-2 hover:underline"
							title="An order's line fills it: received with the order">On order</a
						>
					{:else}
						<button class={link} onclick={() => start(w)}
							>{open === w.id ? 'Close' : 'Bought'}</button
						>
						<button class={link} onclick={() => remove(w)}>Remove</button>
					{/if}
				</span>
			</div>
			{#if open === w.id}
				<form onsubmit={(e) => bought(w, e)} class="mt-2 flex flex-wrap items-end gap-2">
					<label class="text-xs text-muted-foreground"
						>Arrived ({weightUnit()})<WeightInput
							bind:grams={weight}
							required
							class="{field} mt-0.5 block w-24"
						/></label
					>
					<label class="text-xs text-muted-foreground"
						>Price<input
							inputmode="decimal"
							bind:value={price}
							class="{field} mt-0.5 block w-24"
						/></label
					>
					<label class="text-xs text-muted-foreground"
						>Shipping<input
							inputmode="decimal"
							bind:value={shipping}
							placeholder="0"
							class="{field} mt-0.5 block w-20"
						/></label
					>
					<label class="text-xs text-muted-foreground"
						>Date<input type="date" bind:value={date} class="{field} mt-0.5 block w-36" /></label
					>
					<button type="submit" class={secondary} disabled={busy}>Record the purchase</button>
				</form>
			{/if}
		</li>
	{/each}
</ul>
