<script lang="ts">
	/** Beans: roasted coffee resting and ready, the planned roasts queue, the green stock and what it cost (docs/roasted.md, docs/batches.md, docs/green-coffee.md). */
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import { goto, refreshAll } from '$app/navigation';
	import {
		beans as beansApi,
		type Bean,
		type BeanRequest,
		type CostReport,
		type OrderRequest
	} from '#lib/api/beans.js';
	import type { PurchaseOrder } from '#lib/api/orders.js';
	import { plan as planApi, type PlannedRoast, type PlanRequest } from '#lib/api/plans.js';
	import type { RoastProfile } from '#lib/api/profiles.js';
	import type { RoastedStock } from '#lib/api/roasted.js';
	import type { Wanted } from '#lib/api/wanted.js';
	import ImportLink from '#lib/components/beans/ImportLink.svelte';
	import RoastedCoffee from '#lib/components/beans/RoastedCoffee.svelte';
	import ShoppingList from '#lib/components/beans/ShoppingList.svelte';
	import CartLinks from '#lib/components/beans/CartLinks.svelte';
	import { fmtMoney, fmtPerKg } from '#lib/beans/money.js';
	import { bigUnit, perBigUnit } from '#lib/beans/weight.js';
	import { weightState } from '#lib/stores/units.svelte.js';
	import OrderForm from '#lib/components/beans/OrderForm.svelte';
	import { notifications } from '#lib/stores/notifications.js';
	import { roast } from '#lib/stores/roast.svelte.js';
	import {
		fmtGrams,
		levelClass,
		moveId,
		planLabel,
		queueStock,
		stockNote
	} from '#lib/beans/stock.js';
	import BeanForm from '#lib/components/beans/BeanForm.svelte';
	import PlanForm from '#lib/components/beans/PlanForm.svelte';
	import { ArrowUp, ArrowDown, Pencil, Trash2, X } from '@lucide/svelte';
	import { tick } from 'svelte';
	import { isOn } from '#lib/stores/modules.svelte.js';
	import { lineSummary, orderName, statusText } from '#lib/beans/orders.js';

	let { data }: PageProps = $props();
	// svelte-ignore state_referenced_locally
	let beanList = $state<Bean[]>(data.beans);
	// svelte-ignore state_referenced_locally
	let plans = $state<PlannedRoast[]>(data.plans);
	const profiles = $derived<RoastProfile[]>(data.profiles);
	const costs = $derived<CostReport | null>(data.costs);
	const wantedList = $derived<Wanted[]>(data.wanted);
	const roastedStock = $derived<RoastedStock | null>(data.roasted);
	const openOrders = $derived<PurchaseOrder[]>(data.orders);
	const home = $derived(costs?.currency ?? 'USD');
	const suppliers = $derived(
		[...new Set(beanList.map((b) => b.supplier).filter((s): s is string => !!s))].sort()
	);
	const anyCost = $derived(isOn('costs') && beanList.some((b) => b.cost_per_kg != null));
	const monthName = (m: string) =>
		new Date(`${m}-15T12:00:00Z`).toLocaleDateString(undefined, {
			month: 'short',
			year: '2-digit'
		});
	$effect(() => {
		beanList = data.beans;
		plans = data.plans;
	});

	const active = $derived(beanList.filter((b) => !b.archived));
	const archived = $derived(beanList.filter((b) => b.archived));
	const queue = $derived(queueStock(plans));
	const lowCount = $derived(active.filter((b) => b.low).length);
	let showArchived = $state(false);
	let showCarts = $state(false);

	type Form = 'bean' | 'plan' | 'order' | 'link';
	let adding = $state<Form | null>(null);
	const FORM_TITLE: Record<Form, string> = {
		link: 'Add from a link',
		bean: 'Add bean',
		order: 'Record an order',
		plan: 'Plan a roast'
	};
	let formEl = $state<HTMLElement | undefined>();
	/** Open a form (or close it again): it shows under the buttons, in view, its first field focused. */
	function toggle(f: Form) {
		adding = adding === f ? null : f;
		if (!adding) return;
		void tick().then(() => {
			formEl?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
			formEl
				?.querySelector<HTMLElement>('input:not([type=hidden]), select, textarea')
				?.focus({ preventScroll: true });
		});
	}

	async function done(message?: string) {
		await refreshAll();
		if (message) notifications.add(message, 'success');
	}
	let editingPlan = $state<string | null>(null);
	let busy = $state(false);

	async function act<T>(what: string, fn: () => Promise<T>): Promise<T | undefined> {
		busy = true;
		try {
			const r = await fn();
			await refreshAll();
			return r;
		} catch (e) {
			notifications.add(`${what}: ${e instanceof Error ? e.message : e}`, 'error');
			return undefined;
		} finally {
			busy = false;
		}
	}

	async function addBean(req: BeanRequest) {
		const b = await act('Could not add the bean', () => beansApi.create(req));
		if (b) {
			adding = null;
			notifications.add(`Added ${b.name}`, 'success');
		}
	}

	async function addOrder(req: OrderRequest) {
		const r = await act('Could not record the order', () => beansApi.order(req));
		if (r) {
			adding = null;
			notifications.add(
				`Recorded ${r.moves.length} purchase${r.moves.length === 1 ? '' : 's'}`,
				'success'
			);
		}
	}

	async function addPlan(req: PlanRequest) {
		if (await act('Could not plan the roast', () => planApi.create(req))) adding = null;
	}

	async function savePlan(id: string, req: PlanRequest) {
		if (await act('Could not save the planned roast', () => planApi.update(id, req)))
			editingPlan = null;
	}

	async function removePlan(p: PlannedRoast) {
		if (!confirm(`Remove "${planLabel(p)}" from the plan?`)) return;
		await act('Could not remove it', () => planApi.delete(p.id));
	}

	async function move(id: string, by: -1 | 1) {
		const ids = moveId(
			plans.filter((p) => !p.session_id).map((p) => p.id),
			id,
			by
		);
		await act('Could not reorder', () => planApi.reorder(ids));
	}

	const roasterName = (id: string | null) =>
		id ? (roast.devices.find((d) => d.device_id === id)?.name ?? id) : 'Any roaster';
	const made = (b: Bean) => [b.origin, b.variety, b.process].filter(Boolean).join(' · ');

	const th = 'px-3 py-2 text-left text-xs font-medium text-muted-foreground';
	const iconBtn =
		'rounded p-1 text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-30';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
	// The button whose form is open
	const open = 'border-selected bg-accent';
</script>

<svelte:head>
	<title>Beans | Roastwarden</title>
</svelte:head>

<div class="space-y-6">
	<div class="flex flex-wrap items-end justify-between gap-3">
		<div>
			<h1 class="text-2xl font-bold text-foreground">Beans</h1>
			<p class="mt-1 text-sm text-muted-foreground">
				What is roasted and ready to drink, planned roasts and green stock. A roast from a bean
				takes its green weight off the stock when it starts; once weighed, its coffee rests here
				until it is ready.
			</p>
		</div>
		<div class="flex flex-wrap gap-2">
			<button
				class="{secondary} {adding === 'link' ? open : ''}"
				aria-pressed={adding === 'link'}
				aria-controls="beans-form"
				onclick={() => toggle('link')}>Add from a link</button
			>
			<button
				class="{secondary} {adding === 'bean' ? open : ''}"
				aria-pressed={adding === 'bean'}
				aria-controls="beans-form"
				onclick={() => toggle('bean')}>Add bean</button
			>
			{#if isOn('costs')}<button
					class="{secondary} {adding === 'order' ? open : ''}"
					aria-pressed={adding === 'order'}
					aria-controls="beans-form"
					onclick={() => toggle('order')}
					disabled={active.length === 0}>Record an order</button
				>{/if}
			<button
				class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover {adding ===
				'plan'
					? 'ring-2 ring-selected ring-offset-2 ring-offset-background'
					: ''}"
				aria-pressed={adding === 'plan'}
				aria-controls="beans-form"
				onclick={() => toggle('plan')}
			>
				Plan a roast
			</button>
		</div>
	</div>

	<!-- Every form the buttons open shows here, right under them, so it is in view at once -->
	{#if adding}
		<section
			id="beans-form"
			bind:this={formEl}
			aria-label={FORM_TITLE[adding]}
			class="rounded-md border border-selected bg-card p-3"
		>
			<div class="mb-2 flex items-center justify-between gap-2">
				<h2 class="text-sm font-semibold text-foreground">{FORM_TITLE[adding]}</h2>
				<button class={iconBtn} onclick={() => (adding = null)} title="Close" aria-label="Close"
					><X class="h-4 w-4" /></button
				>
			</div>
			{#if adding === 'link'}
				<ImportLink
					beans={beanList}
					currency={home}
					ondone={(m) => {
						adding = null;
						void done(m);
					}}
					oncancel={() => (adding = null)}
				/>
			{:else if adding === 'bean'}
				<BeanForm {busy} onsave={addBean} oncancel={() => (adding = null)} />
			{:else if adding === 'order'}
				<OrderForm
					beans={beanList}
					{busy}
					currency={home}
					{suppliers}
					onsave={addOrder}
					oncancel={() => (adding = null)}
				/>
			{:else if adding === 'plan'}
				<PlanForm
					beans={beanList}
					{profiles}
					{busy}
					onsave={addPlan}
					oncancel={() => (adding = null)}
				/>
			{/if}
		</section>
	{/if}

	<RoastedCoffee stock={roastedStock} onchange={() => refreshAll()} />

	{#if wantedList.length && isOn('costs')}
		<section aria-labelledby="want-h" class="space-y-2">
			<div class="flex flex-wrap items-baseline gap-x-3">
				<h2 id="want-h" class="text-lg font-semibold text-foreground">Shopping list</h2>
				<span class="text-xs text-muted-foreground">"Bought" adds the bean and its purchase.</span>
				<button
					class="ml-auto text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
					onclick={() => (showCarts = !showCarts)}
					title="One link per supplier that opens its cart with these bags">Carts</button
				>
				{#if isOn('reports')}<a
						href={resolve('/reports?kind=shopping')}
						class="text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
						>As a PDF</a
					>{/if}
			</div>
			{#if showCarts}<CartLinks currency={home} onclose={() => (showCarts = false)} />{/if}
			<ShoppingList
				items={wantedList}
				currency={home}
				onchange={(m) => done(m)}
				onerror={(m) => notifications.add(m, 'error')}
			/>
		</section>
	{/if}

	{#if openOrders.length}
		<section aria-labelledby="orders-h" class="space-y-2">
			<div class="flex flex-wrap items-baseline gap-x-3">
				<h2 id="orders-h" class="text-lg font-semibold text-foreground">On order</h2>
				<span class="text-xs text-muted-foreground">From the receipts: stock when received.</span>
				<a
					href={resolve('/beans/orders')}
					class="ml-auto text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
					>All orders</a
				>
			</div>
			<ul class="divide-y divide-border rounded-md border border-border bg-card">
				{#each openOrders as o (o.id)}
					<li>
						<a
							href={resolve('/beans/orders/[id]', { id: o.id })}
							class="flex flex-wrap items-baseline gap-x-3 px-3 py-2 hover:bg-accent"
						>
							<span class="font-medium text-foreground">{orderName(o)}</span>
							<span class="text-xs text-muted-foreground"
								>{lineSummary(o.lines)} · {fmtGrams(o.coffee_g)}{isOn('costs')
									? ` · ${fmtMoney(o.total, o.currency)}`
									: ''}</span
							>
							<span class="ml-auto text-xs text-highlight"
								>{statusText(o.status)}{o.tracking
									? ` · ${o.carrier ?? ''} ${o.tracking}`
									: ''}</span
							>
						</a>
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	<!-- Planned roasts -->
	<section aria-labelledby="plan-h" class="space-y-2">
		<div class="flex items-baseline gap-3">
			<h2 id="plan-h" class="text-lg font-semibold text-foreground">Planned roasts</h2>
			<span class="text-xs text-muted-foreground"
				>In order: the Roast page offers the first one for its roaster.</span
			>
			{#if isOn('reports')}<a
					href={resolve('/reports?kind=plan')}
					class="ml-auto text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
					>Roast-day sheet as PDF</a
				>{/if}
		</div>
		{#if plans.length === 0}
			<p
				class="rounded-md border border-dashed border-border px-3 py-4 text-sm text-muted-foreground"
			>
				Nothing planned. "Plan a roast" queues bean, weight and profile; the Roast page then starts
				it in one tap.
			</p>
		{:else}
			<ol class="divide-y divide-border rounded-md border border-border bg-card">
				{#each plans as p, i (p.id)}
					{@const q = queue.get(p.id)}
					<li class="px-3 py-2">
						{#if editingPlan === p.id}
							<PlanForm
								plan={p}
								beans={beanList}
								{profiles}
								{busy}
								onsave={(r) => savePlan(p.id, r)}
								oncancel={() => (editingPlan = null)}
							/>
						{:else}
							<div class="flex items-center gap-2">
								<span class="w-5 shrink-0 text-right text-xs tabular-nums text-muted-foreground"
									>{i + 1}</span
								>
								<div class="min-w-0 flex-1">
									<div class="truncate text-sm font-medium text-foreground">{planLabel(p)}</div>
									<div class="flex flex-wrap gap-x-3 text-xs text-muted-foreground">
										<span>{roasterName(p.device_id)}</span>
										{#if q}
											<span class={q.short ? 'text-red-400' : 'text-muted-foreground'}>
												{q.short
													? `Not enough ${p.bean_name ?? 'beans'}: ${fmtGrams(-q.leftG)} short by then`
													: `${fmtGrams(q.leftG)} left after it`}
											</span>
										{/if}
										{#if p.notes}<span class="truncate">{p.notes}</span>{/if}
									</div>
								</div>
								<button
									class={iconBtn}
									onclick={() => move(p.id, -1)}
									disabled={busy || i === 0}
									title="Earlier"
									aria-label="Move earlier"><ArrowUp class="h-4 w-4" /></button
								>
								<button
									class={iconBtn}
									onclick={() => move(p.id, 1)}
									disabled={busy || i === plans.length - 1}
									title="Later"
									aria-label="Move later"><ArrowDown class="h-4 w-4" /></button
								>
								<button
									class={iconBtn}
									onclick={() => (editingPlan = p.id)}
									title="Edit"
									aria-label="Edit"><Pencil class="h-4 w-4" /></button
								>
								<button
									class={iconBtn}
									onclick={() => removePlan(p)}
									disabled={busy}
									title="Remove"
									aria-label="Remove"><Trash2 class="h-4 w-4" /></button
								>
							</div>
						{/if}
					</li>
				{/each}
			</ol>
		{/if}
	</section>

	<!-- Stock -->
	<section aria-labelledby="stock-h" class="space-y-2">
		<div class="flex items-baseline gap-3">
			<h2 id="stock-h" class="text-lg font-semibold text-foreground">Stock</h2>
			{#if lowCount}<span class="text-xs text-amber-400">{lowCount} running low</span>{/if}
			<a
				href={resolve('/beans/orders')}
				class="ml-auto text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
				title="Supplier orders recorded from their receipts">Orders</a
			>
			{#if isOn('reports')}<a
					href={resolve('/reports?kind=inventory')}
					class=" text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
					>Inventory and stock-take as PDF</a
				>{/if}
		</div>
		{#if active.length === 0}
			<p
				class="rounded-md border border-dashed border-border px-3 py-4 text-sm text-muted-foreground"
			>
				No beans yet. "Add bean" with what is in the bag; a roast from it then counts it down.
			</p>
		{:else}
			<!-- Phone: cards -->
			<ul class="divide-y divide-border rounded-md border border-border bg-card sm:hidden">
				{#each active as b (b.id)}
					{@const note = stockNote(b, null)}
					<li>
						<a href={resolve('/beans/[id]', { id: b.id })} class="block px-3 py-2.5">
							<div class="flex items-baseline justify-between gap-2">
								<span class="font-medium text-foreground">{b.name}</span>
								<span class="tabular-nums {b.low ? 'text-amber-400' : 'text-foreground'}"
									>{fmtGrams(b.stock_g)}{b.on_order_g
										? ` +${fmtGrams(b.on_order_g)} on order`
										: ''}</span
								>
							</div>
							<div class="text-xs text-muted-foreground">
								{made(b) || '—'}{b.low
									? ` · ${note.level === 'out' ? 'out of stock' : 'low'}`
									: ''}{anyCost && b.cost_per_kg != null
									? ` · ${fmtPerKg(b.cost_per_kg, b.currency)}`
									: ''}
							</div>
						</a>
					</li>
				{/each}
			</ul>
			<!-- Wider: a table -->
			<div class="hidden overflow-x-auto rounded-md border border-border bg-card sm:block">
				<table class="w-full text-sm">
					<thead class="border-b border-border">
						<tr>
							<th class={th}>Bean</th>
							<th class={th}>Lot</th>
							<th class={th}>Supplier</th>
							<th class="{th} text-right">Stock</th>
							{#if anyCost}<th class="{th} text-right">Cost/{bigUnit(weightState.unit)}</th>{/if}
							<th class="{th} text-right">Roasts</th>
						</tr>
					</thead>
					<tbody class="divide-y divide-border">
						{#each active as b (b.id)}
							<tr
								class="cursor-pointer hover:bg-accent"
								onclick={() => goto(resolve('/beans/[id]', { id: b.id }))}
							>
								<td class="px-3 py-2">
									<a
										href={resolve('/beans/[id]', { id: b.id })}
										class="font-medium text-foreground hover:underline">{b.name}</a
									>
									{#if made(b)}<div class="text-xs text-muted-foreground">{made(b)}</div>{/if}
								</td>
								<td class="px-3 py-2 text-xs text-muted-foreground">{b.lot ?? '—'}</td>
								<td class="px-3 py-2 text-xs text-muted-foreground">{b.supplier ?? '—'}</td>
								<td class="px-3 py-2 text-right tabular-nums">
									<span
										class={b.low
											? levelClass(stockNote(b, null).level === 'out' ? 'out' : 'low')
											: 'text-foreground'}>{fmtGrams(b.stock_g)}</span
									>
									{#if b.low && !(b.stock_g <= 0 && b.on_order_g)}<div
											class="text-[11px] text-amber-400"
										>
											{b.stock_g <= 0 ? 'out of stock' : `low (below ${fmtGrams(b.low_stock_g)})`}
										</div>{/if}
									{#if b.on_order_g}<div class="text-[11px] text-highlight">
											+{fmtGrams(b.on_order_g)} on order
										</div>{/if}
								</td>
								{#if anyCost}<td
										class="px-3 py-2 text-right text-xs tabular-nums text-muted-foreground"
										>{b.cost_per_kg != null
											? fmtMoney(perBigUnit(b.cost_per_kg, weightState.unit), b.currency)
											: '—'}</td
									>{/if}
								<td class="px-3 py-2 text-right tabular-nums text-muted-foreground"
									>{b.roast_count}</td
								>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
		{#if archived.length}
			<button
				class="text-xs text-muted-foreground hover:text-foreground"
				onclick={() => (showArchived = !showArchived)}
			>
				{showArchived ? 'Hide' : 'Show'}
				{archived.length} archived
			</button>
			{#if showArchived}
				<ul class="divide-y divide-border rounded-md border border-border bg-card text-sm">
					{#each archived as b (b.id)}
						<li class="flex items-baseline justify-between px-3 py-2">
							<a
								href={resolve('/beans/[id]', { id: b.id })}
								class="text-muted-foreground hover:text-foreground hover:underline">{b.name}</a
							>
							<span class="text-xs tabular-nums text-muted-foreground"
								>{fmtGrams(b.stock_g)} · {b.roast_count} roasts</span
							>
						</li>
					{/each}
				</ul>
			{/if}
		{/if}
	</section>

	{#if isOn('costs') && costs && costs.months.some((m) => m.spent > 0 || m.roast_cost > 0 || Object.keys(m.spent_other).length)}
		<!-- What it cost, by month -->
		<section aria-labelledby="cost-h" class="space-y-2">
			<div class="flex flex-wrap items-baseline gap-x-3">
				<h2 id="cost-h" class="text-lg font-semibold text-foreground">Spending</h2>
				<span class="text-xs text-muted-foreground"
					>By month: priced purchases with shipping and fees; "In roasts" is what the green coffee
					of the roasts cost.</span
				>
			</div>
			<div class="overflow-x-auto rounded-md border border-border bg-card">
				<table class="w-full text-sm">
					<thead class="border-b border-border">
						<tr>
							<th class={th}>Month</th>
							<th class="{th} px-2 sm:px-3 text-right">Spent</th>
							<th class="{th} px-2 sm:px-3 text-right">Bought</th>
							<th class="{th} px-2 sm:px-3 text-right">Roasts</th>
							<th class="{th} hidden px-2 sm:px-3 text-right sm:table-cell">Green roasted</th>
							<th class="{th} hidden px-2 sm:px-3 text-right sm:table-cell">Roasted</th>
							<th class="{th} px-2 sm:px-3 text-right">In roasts</th>
						</tr>
					</thead>
					<tbody class="divide-y divide-border whitespace-nowrap tabular-nums">
						{#each costs.months as m (m.month)}
							<tr>
								<td class="px-2 py-1.5 sm:px-3 text-foreground">{monthName(m.month)}</td>
								<td class="px-2 py-1.5 sm:px-3 text-right text-foreground">
									{m.spent > 0 ? fmtMoney(m.spent, costs.currency) : '—'}
									{#each Object.entries(m.spent_other) as [c, v] (c)}<div
											class="text-xs text-muted-foreground"
										>
											+ {fmtMoney(v, c)}
										</div>{/each}
								</td>
								<td class="px-2 py-1.5 sm:px-3 text-right text-muted-foreground"
									>{m.bought_g > 0 ? fmtGrams(m.bought_g) : '—'}</td
								>
								<td class="px-2 py-1.5 sm:px-3 text-right text-muted-foreground"
									>{m.roasts || '—'}</td
								>
								<td
									class="hidden px-2 py-1.5 sm:px-3 text-right text-muted-foreground sm:table-cell"
									>{m.green_roasted_g > 0 ? fmtGrams(m.green_roasted_g) : '—'}</td
								>
								<td
									class="hidden px-2 py-1.5 sm:px-3 text-right text-muted-foreground sm:table-cell"
									>{m.roasted_g > 0 ? fmtGrams(m.roasted_g) : '—'}</td
								>
								<td class="px-2 py-1.5 sm:px-3 text-right text-foreground">
									{m.costed_roasts ? fmtMoney(m.roast_cost, costs.currency) : '—'}
									{#if m.costed_roasts && m.costed_roasts < m.roasts}<div
											class="text-xs text-muted-foreground"
										>
											{m.costed_roasts} of {m.roasts} roasts
										</div>{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</section>
	{/if}
</div>
