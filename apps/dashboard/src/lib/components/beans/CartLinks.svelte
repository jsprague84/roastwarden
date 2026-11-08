<script lang="ts">
	/**
	 * Cart links for the shopping list (docs/green-coffee.md, "Cart links"): per supplier one
	 * link that opens its cart with the bags in it. The server reads each bag again first, a
	 * few seconds apart, so this takes a moment. Nothing is bought here.
	 */
	import { wanted as wantedApi, type Carts } from '#lib/api/wanted.js';
	import { fmtMoney } from '#lib/beans/money.js';

	interface Props {
		currency?: string;
		onclose: () => void;
	}
	let { currency = 'USD', onclose }: Props = $props();

	let carts = $state<Carts | null>(null);
	let error = $state('');
	let reading = $state(true);

	$effect(() => {
		wantedApi
			.carts()
			.then((c) => (carts = c))
			.catch(
				(e) => (error = e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e))
			)
			.finally(() => (reading = false));
	});
</script>

<div class="space-y-3 rounded-md border border-border bg-card p-3 text-sm">
	<div class="flex items-baseline gap-3">
		<h3 class="font-semibold text-foreground">Carts</h3>
		<span class="text-xs text-muted-foreground"
			>Each opens the supplier's cart with your bags in it; you check out there.</span
		>
		<button
			class="ml-auto text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
			onclick={onclose}>Close</button
		>
	</div>
	{#if reading}
		<p class="text-muted-foreground">Reading each coffee from its store again…</p>
	{:else if error}
		<p class="text-red-400">{error}</p>
	{:else if carts}
		{#each carts.carts as c (c.host)}
			<div class="space-y-1 border-t border-border pt-2 first:border-t-0 first:pt-0">
				<div class="flex flex-wrap items-baseline gap-x-3">
					<span class="font-medium text-foreground">{c.supplier}</span>
					<span class="text-xs text-muted-foreground"
						>{c.lines.filter((l) => l.in_link).length} in the cart · {fmtMoney(
							c.total,
							c.currency ?? currency
						)} before shipping</span
					>
					{#if c.url}
						<a
							href={c.url}
							target="_blank"
							rel="external noopener noreferrer"
							class="ml-auto rounded-md bg-action px-3 py-1 text-xs font-medium text-action-foreground hover:bg-action-hover"
							>Open cart at {c.supplier}</a
						>
					{/if}
				</div>
				<ul class="text-xs text-muted-foreground">
					{#each c.lines as l (l.wanted_id)}
						<li class={l.in_link ? '' : 'line-through decoration-muted-foreground/60'}>
							{l.name}{l.size_label ? `, ${l.size_label}` : ''} × {l.quantity}{l.price != null
								? ` at ${fmtMoney(l.price, c.currency ?? currency)}`
								: ''}{l.note ? ` (${l.note})` : ''}
						</li>
					{/each}
				</ul>
				{#each c.warnings as w (w)}<p class="text-xs text-amber-400">{w}</p>{/each}
			</div>
		{:else}
			<p class="text-muted-foreground">Nothing on the list can go in a cart.</p>
		{/each}
		{#each carts.left_out as l (l.wanted_id)}
			<p class="text-xs text-muted-foreground">Left out: {l.name} ({l.why})</p>
		{/each}
	{/if}
</div>
