<script lang="ts">
	/**
	 * One order from a supplier: a line per bean (weight, price), one shipping and fee total
	 * split over the lines by weight (docs/green-coffee.md, POST /api/purchases).
	 */
	import type { Bean, OrderRequest } from '#lib/api/beans.js';
	import { moneyOrNull, splitByWeight } from '#lib/beans/money.js';
	import { weightUnit } from '#lib/stores/units.svelte.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';

	interface Props {
		beans: Bean[];
		busy?: boolean;
		currency?: string;
		suppliers?: string[];
		onsave: (req: OrderRequest) => void;
		oncancel: () => void;
	}
	let { beans, busy = false, currency = 'USD', suppliers = [], onsave, oncancel }: Props = $props();

	interface Line {
		bean_id: string;
		weight: number | null;
		price: number | string | null;
	}
	let supplier = $state('');
	let date = $state('');
	let cur = $state('');
	let shipping = $state<number | string | null>('');
	let fees = $state<number | string | null>('');
	let note = $state('');
	let lines = $state<Line[]>([{ bean_id: '', weight: null, price: '' }]);

	const weights = $derived(lines.map((l) => l.weight ?? 0));
	const shipShares = $derived(splitByWeight(moneyOrNull(shipping), weights));
	const feeShares = $derived(splitByWeight(moneyOrNull(fees), weights));
	const ready = $derived(lines.length > 0 && lines.every((l) => l.bean_id && (l.weight ?? 0) > 0));
	const total = $derived(
		lines.reduce((a, l) => a + (moneyOrNull(l.price) ?? 0), 0) +
			(moneyOrNull(shipping) ?? 0) +
			(moneyOrNull(fees) ?? 0)
	);
	const code = $derived((cur.trim() || currency).toUpperCase());

	function submit(e: SubmitEvent) {
		e.preventDefault();
		if (!ready) return;
		onsave({
			supplier: supplier.trim() || null,
			currency: cur.trim() ? cur.trim().toUpperCase() : null,
			shipping: moneyOrNull(shipping),
			fees: moneyOrNull(fees),
			at: date || null,
			note: note.trim() || null,
			lines: lines.map((l) => ({
				bean_id: l.bean_id,
				weight_g: l.weight ?? 0,
				price: moneyOrNull(l.price)
			}))
		});
	}

	const field =
		'rounded-md border border-border bg-input px-2.5 py-1.5 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const label = 'text-xs text-muted-foreground';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
</script>

<form onsubmit={submit} class="space-y-3">
	<div class="flex flex-wrap items-end gap-2">
		<label class={label}
			>Supplier<input
				bind:value={supplier}
				list="order-suppliers"
				class="{field} mt-0.5 block w-44"
			/></label
		>
		<label class={label}
			>Date<input type="date" bind:value={date} class="{field} mt-0.5 block w-36" /></label
		>
		<label class={label}
			>Shipping<input
				inputmode="decimal"
				bind:value={shipping}
				placeholder="0"
				class="{field} mt-0.5 block w-20"
			/></label
		>
		<label class={label}
			>Fees, taxes<input
				inputmode="decimal"
				bind:value={fees}
				placeholder="0"
				class="{field} mt-0.5 block w-20"
			/></label
		>
		<label class={label}
			>Currency<input
				bind:value={cur}
				maxlength="3"
				placeholder={currency}
				class="{field} mt-0.5 block w-16 uppercase"
			/></label
		>
		<label class={label}
			>Note<input
				bind:value={note}
				placeholder="order number"
				class="{field} mt-0.5 block w-40"
			/></label
		>
	</div>
	<table class="text-sm">
		<thead>
			<tr class="text-left text-xs text-muted-foreground">
				<th class="pb-1 pr-2 font-medium">Bean</th>
				<th class="pb-1 pr-2 font-medium">Weight ({weightUnit()})</th>
				<th class="pb-1 pr-2 font-medium">Price</th>
				<th class="pb-1 pr-2 font-medium">Shipping and fees share</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each lines as l, i (i)}
				<tr>
					<td class="py-0.5 pr-2">
						<select bind:value={l.bean_id} class="{field} w-56" aria-label="Bean" required>
							<option value="" disabled>Choose a bean</option>
							{#each beans.filter((b) => !b.archived) as b (b.id)}<option value={b.id}
									>{b.name}</option
								>{/each}
						</select>
					</td>
					<td class="py-0.5 pr-2"
						><WeightInput
							bind:grams={l.weight}
							class="{field} w-24"
							aria-label="Weight ({weightUnit()})"
							required
						/></td
					>
					<td class="py-0.5 pr-2"
						><input
							inputmode="decimal"
							bind:value={l.price}
							placeholder={code}
							class="{field} w-24"
							aria-label="Price"
						/></td
					>
					<td class="py-0.5 pr-2 text-xs tabular-nums text-muted-foreground">
						{#if shipShares[i] != null || feeShares[i] != null}{(
								(shipShares[i] ?? 0) + (feeShares[i] ?? 0)
							).toFixed(2)}
							{code}{/if}
					</td>
					<td class="py-0.5">
						{#if lines.length > 1}<button
								type="button"
								class="text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
								onclick={() => (lines = lines.filter((_, j) => j !== i))}>Remove</button
							>{/if}
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
	<div class="flex flex-wrap items-center gap-2">
		<button
			type="button"
			class={secondary}
			onclick={() => (lines = [...lines, { bean_id: '', weight: null, price: '' }])}
			>Add a line</button
		>
		<button
			type="submit"
			class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
			disabled={busy || !ready}>Record the order</button
		>
		<button type="button" class={secondary} onclick={oncancel}>Cancel</button>
		{#if total > 0}<span class="text-xs tabular-nums text-muted-foreground"
				>{total.toFixed(2)} {code} in all</span
			>{/if}
	</div>
	<p class="text-xs text-muted-foreground">
		A bean not listed yet: add it first ("Add bean"), then record the order.
	</p>
	<datalist id="order-suppliers">
		{#each suppliers as s (s)}<option value={s}></option>{/each}
	</datalist>
</form>
