<script lang="ts">
	/**
	 * Add from a supplier's link (docs/green-coffee-sources.md): the server reads the product
	 * page and returns a preview; nothing is saved until one of the buttons. A bean with the
	 * same link (or name) is offered to buy again instead of making a second one.
	 */
	import { beans as beansApi, type Bean } from '#lib/api/beans.js';
	import { wanted as wantedApi, type Draft } from '#lib/api/wanted.js';
	import { fmtGrams } from '#lib/beans/stock.js';
	import { fmtMoney, moneyOrNull } from '#lib/beans/money.js';
	import { isOn } from '#lib/stores/modules.svelte.js';

	interface Props {
		beans: Bean[];
		currency?: string;
		/** After something was saved: a line to show. */
		ondone: (message: string) => void;
		oncancel: () => void;
	}
	let { beans, currency = 'USD', ondone, oncancel }: Props = $props();

	let link = $state('');
	let draft = $state<Draft | null>(null);
	let size = $state('');
	let shipping = $state<number | string | null>('');
	let busy = $state(false);
	let error = $state('');
	let again = $state('');

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	async function preview(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		draft = null;
		try {
			const d = await beansApi.importLink(link.trim());
			draft = d;
			size = d.sizes.find((z) => z.available !== false)?.label ?? d.sizes[0]?.label ?? '';
			const same = beans.find(
				(b) =>
					!b.archived &&
					((b.url && b.url === d.url) || b.name.toLowerCase() === d.name.toLowerCase())
			);
			again = same?.id ?? '';
		} catch (err) {
			error = msg(err);
		} finally {
			busy = false;
		}
	}

	async function run(fn: () => Promise<string>) {
		busy = true;
		error = '';
		try {
			ondone(await fn());
		} catch (err) {
			error = msg(err);
		} finally {
			busy = false;
		}
	}

	const addBean = () =>
		run(async () => {
			const d = draft!;
			const b = await beansApi.create({
				...d,
				details: Object.keys(d.details).length ? d.details : null
			});
			return `Added ${b.name}`;
		});

	const want = () =>
		run(async () => {
			const w = await wantedApi.add({
				draft: draft!,
				size: size || undefined,
				bean_id: again || undefined
			});
			return `${w.name}${w.size_label ? `, ${w.size_label}` : ''} is on the shopping list`;
		});

	const bought = () =>
		run(async () => {
			const w = await wantedApi.add({
				draft: draft!,
				size: size || undefined,
				bean_id: again || undefined
			});
			const r = await wantedApi.bought(w.id, { shipping: moneyOrNull(shipping) ?? undefined });
			return `Bought ${w.size_label ?? ''} of ${r.bean.name}: ${fmtGrams(r.bean.stock_g)} in stock`;
		});

	const facts = $derived(
		draft
			? [
					['Origin', [draft.origin, draft.region].filter(Boolean).join(' · ')],
					['Producer', draft.producer],
					['Variety', draft.variety],
					['Process', [draft.process, draft.drying].filter(Boolean).join(' · ')],
					['Altitude', draft.altitude],
					['Grade', draft.grade],
					['Arrival', draft.arrival],
					['Score', draft.score != null ? String(draft.score) : null],
					['Roast', draft.roast_recommendation],
					['Lot', draft.lot]
				].filter((f): f is [string, string] => !!f[1])
			: []
	);

	const field =
		'rounded-md border border-border bg-input px-2.5 py-1.5 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
</script>

<div class="space-y-3">
	<form onsubmit={preview} class="flex flex-wrap items-end gap-2">
		<label class="min-w-0 flex-1 text-xs text-muted-foreground">
			A product page at Sweet Maria's, Bodhi Leaf, Theta Ridge, Coffee Bean Corral, Showroom, Happy
			Mug or Burman
			<input
				type="url"
				bind:value={link}
				required
				placeholder="https://www.sweetmarias.com/products/…"
				class="{field} mt-0.5 block w-full"
			/>
		</label>
		<button type="submit" class={secondary} disabled={busy || !link.trim()}
			>{busy && !draft ? 'Reading…' : 'Preview'}</button
		>
		<button type="button" class={secondary} onclick={oncancel}>Close</button>
	</form>
	{#if error}<p class="text-sm text-red-400 [overflow-wrap:anywhere]" role="alert">{error}</p>{/if}

	{#if draft}
		<div class="space-y-3 border-t border-border pt-3">
			<div>
				<h3 class="font-semibold text-foreground">{draft.name}</h3>
				<p class="text-xs text-muted-foreground">{draft.supplier} · nothing is saved yet</p>
			</div>
			{#if facts.length}
				<dl class="grid grid-cols-2 gap-x-6 gap-y-1 text-sm sm:grid-cols-4">
					{#each facts as [k, v] (k)}
						<div>
							<dt class="text-xs text-muted-foreground">{k}</dt>
							<dd class="text-foreground">{v}</dd>
						</div>
					{/each}
				</dl>
			{/if}
			{#if draft.cupping_notes}<p class="max-w-3xl text-sm text-muted-foreground">
					{draft.cupping_notes}
				</p>{/if}
			{#if draft.sizes.length}
				<fieldset class="text-sm">
					<legend class="mb-1 text-xs text-muted-foreground">Bag size</legend>
					<div class="flex flex-wrap gap-x-4 gap-y-1">
						{#each draft.sizes as z (z.label)}
							<label
								class="flex items-center gap-1.5 {z.available === false
									? 'text-muted-foreground'
									: 'text-foreground'}"
							>
								<input type="radio" bind:group={size} value={z.label} />
								{z.label}
								<span class="tabular-nums text-muted-foreground"
									>{fmtMoney(z.price, z.currency ?? currency)}{z.available === false
										? ' · sold out'
										: ''}</span
								>
							</label>
						{/each}
					</div>
				</fieldset>
			{/if}
			<div class="flex flex-wrap items-end gap-2">
				<label class="text-xs text-muted-foreground">
					Buy for
					<select bind:value={again} class="{field} mt-0.5 block">
						<option value="">A new bean from this page</option>
						{#each beans.filter((b) => !b.archived) as b (b.id)}<option value={b.id}
								>{b.name}</option
							>{/each}
					</select>
				</label>
				{#if isOn('costs')}
					<button
						class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
						onclick={want}
						disabled={busy}>Put on the shopping list</button
					>
					<label class="text-xs text-muted-foreground"
						>Shipping<input
							inputmode="decimal"
							bind:value={shipping}
							placeholder="0"
							class="{field} mt-0.5 block w-20"
						/></label
					>
				{/if}
				<button class={secondary} onclick={bought} disabled={busy || !size}>Bought it</button>
				{#if !again}<button class={secondary} onclick={addBean} disabled={busy}
						>Only add the bean</button
					>{/if}
			</div>
		</div>
	{/if}
</div>
