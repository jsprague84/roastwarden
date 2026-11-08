<script lang="ts">
	/**
	 * One purchase: weight, price, shipping, supplier and date, with fees, currency, crop
	 * year and arrival folded under "More" (docs/green-coffee.md). Adds or saves an edit.
	 */
	import { fmtPerKg, moneyOrNull, type PurchaseDraft } from '#lib/beans/money.js';
	import { weightUnit } from '#lib/stores/units.svelte.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';
	import { isOn } from '#lib/stores/modules.svelte.js';

	interface Props {
		draft: PurchaseDraft;
		busy?: boolean;
		submitLabel?: string;
		/** The home currency, shown as the placeholder. */
		currency?: string;
		/** Suggestions for the supplier field. */
		suppliers?: string[];
		/** Weight may be negative (editing a correction). */
		correction?: boolean;
		onsubmit: () => void;
		oncancel?: () => void;
		ondelete?: () => void;
	}
	let {
		draft = $bindable(),
		busy = false,
		submitLabel = 'Add purchase',
		currency = 'USD',
		suppliers = [],
		correction = false,
		onsubmit,
		oncancel,
		ondelete
	}: Props = $props();

	let more = $state(!!(draft.fees || draft.currency || draft.crop_year || draft.arrival));
	const uid = $props.id();
	const weightOk = $derived.by(() => {
		const g = draft.weight;
		return g != null && g !== 0 && (correction || g > 0);
	});
	const moneyBad = $derived(
		(['price', 'shipping', 'fees'] as const).some(
			(k) => draft[k] !== '' && draft[k] != null && moneyOrNull(draft[k]) == null
		)
	);
	const perKg = $derived.by(() => {
		const g = draft.weight;
		const p = moneyOrNull(draft.price);
		if (!g || g <= 0 || p == null) return null;
		const total = p + (moneyOrNull(draft.shipping) ?? 0) + (moneyOrNull(draft.fees) ?? 0);
		return total / (g / 1000);
	});

	function submit(e: SubmitEvent) {
		e.preventDefault();
		if (weightOk && !moneyBad) onsubmit();
	}

	const field =
		'rounded-md border border-border bg-input px-2.5 py-1.5 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const label = 'text-xs text-muted-foreground';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
</script>

<form onsubmit={submit} class="space-y-2">
	<div class="flex flex-wrap items-end gap-2">
		<label class={label}
			>{correction ? 'Change' : 'Weight'} ({weightUnit()})<WeightInput
				signed={correction}
				bind:grams={draft.weight}
				class="{field} mt-0.5 block w-24"
				required
			/></label
		>
		{#if !correction}
			{#if isOn('costs')}
				<label class={label}
					>Price<input
						inputmode="decimal"
						bind:value={draft.price}
						placeholder={currency}
						class="{field} mt-0.5 block w-24"
					/></label
				>
				<label class={label}
					>Shipping<input
						inputmode="decimal"
						bind:value={draft.shipping}
						placeholder="0"
						class="{field} mt-0.5 block w-20"
					/></label
				>
			{/if}
			<label class={label}
				>Supplier<input
					bind:value={draft.supplier}
					list="{uid}-suppliers"
					class="{field} mt-0.5 block w-40"
				/></label
			>
			<label class={label}
				>Date<input type="date" bind:value={draft.date} class="{field} mt-0.5 block w-36" /></label
			>
		{/if}
		<label class={label}
			>Note<input
				bind:value={draft.note}
				placeholder="optional"
				class="{field} mt-0.5 block w-40"
			/></label
		>
		{#if !correction}
			<button
				type="button"
				class="pb-2 text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
				onclick={() => (more = !more)}
				aria-expanded={more}>{more ? 'Less' : 'More'}</button
			>
		{/if}
	</div>
	{#if more && !correction}
		<div class="flex flex-wrap items-end gap-2">
			{#if isOn('costs')}
				<label class={label}
					>Fees, taxes<input
						inputmode="decimal"
						bind:value={draft.fees}
						placeholder="0"
						class="{field} mt-0.5 block w-20"
					/></label
				>
				<label class={label}
					>Currency<input
						bind:value={draft.currency}
						maxlength="3"
						placeholder={currency}
						class="{field} mt-0.5 block w-16 uppercase"
					/></label
				>
			{/if}
			<label class={label}
				>Crop year<input
					bind:value={draft.crop_year}
					placeholder="2026"
					class="{field} mt-0.5 block w-24"
				/></label
			>
			<label class={label}
				>Arrival<input
					bind:value={draft.arrival}
					placeholder="2026-07"
					class="{field} mt-0.5 block w-24"
				/></label
			>
		</div>
	{/if}
	<div class="flex flex-wrap items-center gap-2">
		<button type="submit" class={secondary} disabled={busy || !weightOk || moneyBad}
			>{submitLabel}</button
		>
		{#if oncancel}<button type="button" class={secondary} onclick={oncancel}>Cancel</button>{/if}
		{#if ondelete}<button
				type="button"
				class="{secondary} text-red-400"
				onclick={ondelete}
				disabled={busy}>Delete</button
			>{/if}
		{#if perKg != null && isOn('costs')}<span class="text-xs tabular-nums text-muted-foreground"
				>{fmtPerKg(perKg, draft.currency || currency)} with shipping and fees</span
			>{/if}
		{#if moneyBad}<span class="text-xs text-red-400">Money is a number of 0 or more.</span>{/if}
	</div>
	<datalist id="{uid}-suppliers">
		{#each suppliers as s (s)}<option value={s}></option>{/each}
	</datalist>
</form>
