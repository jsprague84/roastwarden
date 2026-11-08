<script lang="ts">
	/** Roasted coffee on the Beans page (docs/roasted.md): what is ready, what is resting, and what was used, given or packed;
	 *  roasts in one bag show as one row, of one coffee or a blend ("Roasts in one bag"). */
	import { resolve } from '$app/paths';
	import {
		roasted as roastedApi,
		type RoastedFor,
		type RoastedMoveRequest,
		type RoastedStock
	} from '#lib/api/roasted.js';
	import { fmtGrams } from '#lib/beans/stock.js';
	import {
		amountHint,
		bagDays,
		bagStateLine,
		blendLine,
		combinable,
		forLabel,
		GRINDS,
		groupLine,
		lotName,
		lotRows,
		parseAmount,
		parseBags,
		stateClass,
		stateLine,
		type LotRow
	} from '#lib/beans/roasted.js';
	import { weightState } from '#lib/stores/units.svelte.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { isOn } from '#lib/stores/modules.svelte.js';

	let { stock, onchange }: { stock: RoastedStock | null; onchange: () => Promise<void> | void } =
		$props();

	let showFinished = $state(false);
	let withFinished = $state<RoastedStock | null>(null);
	const shown = $derived(showFinished && withFinished ? withFinished : stock);
	const lots = $derived(shown?.lots ?? []);
	const rows = $derived(lotRows(lots));
	const groups = $derived(stock?.groups ?? []);

	/** What a row's actions act on: a roast, or a bag through its first roast (the server
	 *  splits a move on any of a bag's roasts across them). */
	type Target = {
		key: string;
		id: string;
		name: string;
		left: number | null | undefined;
		for: RoastedFor;
		active: boolean;
		bag: boolean;
	};
	function target(r: LotRow): Target {
		if (r.kind === 'lot') {
			const l = r.lot;
			return {
				key: l.session_id,
				id: l.session_id,
				name: lotName(l),
				left: l.left_g,
				for: l.for,
				active: l.state !== 'finished' && l.state !== 'unweighed',
				bag: false
			};
		}
		const first = r.members[0];
		return {
			key: r.bag.id,
			id: first.session_id,
			name: `the bag ${r.bag.name}`,
			left: r.bag.left_g,
			for: first.for,
			active: r.bag.state !== 'finished',
			bag: true
		};
	}
	/** The roasts that may join a row: into one bag with the roast, or into the bag */
	const joinable = (r: LotRow) => combinable(r.kind === 'lot' ? r.lot : r.members[0], lots);
	/** The row's coffee (a bag's first roast's) */
	const baseBean = (r: LotRow) => (r.kind === 'lot' ? r.lot.bean_id : r.members[0].bean_id);
	/** The bag would hold more than one coffee: a blend already, or another coffee picked */
	const blendPicked = (r: LotRow) =>
		(r.kind === 'bag' && r.bag.blend) ||
		joinable(r).some((o) => picked.includes(o.session_id) && o.bean_id !== baseBean(r));

	async function toggleFinished() {
		showFinished = !showFinished;
		if (showFinished) {
			try {
				withFinished = await roastedApi.stock(true);
			} catch (e) {
				notifications.add(
					`Could not load finished roasts: ${e instanceof Error ? e.message : e}`,
					'error'
				);
			}
		}
	}

	// One move form open at a time
	type Kind = 'used' | 'given' | 'packed' | 'combine';
	let open = $state<{ id: string; kind: Kind } | null>(null);
	let amount = $state('');
	let to = $state('');
	let note = $state('');
	// Packed: whole bean, or ground with a grind
	let ground = $state(false);
	let grind = $state('');
	// Combine: the roasts picked to go into one bag with this row
	let picked = $state<string[]>([]);
	let bagName = $state('');
	let busy = $state(false);
	let formError = $state('');

	function start(t: Target, kind: Kind) {
		if (open?.id === t.key && open.kind === kind) {
			open = null;
			return;
		}
		open = { id: t.key, kind };
		amount = '';
		to = '';
		note = '';
		ground = false;
		grind = '';
		picked = [];
		bagName = '';
		formError = '';
	}

	async function record(t: Target, req: RoastedMoveRequest, done: string) {
		busy = true;
		try {
			const after = await roastedApi.move(t.id, req);
			open = null;
			const last = after.moves?.at(-1);
			notifications.add(
				`${done}: ${fmtGrams(after.combined?.left_g ?? after.left_g)} left of ${t.name}`,
				'success',
				8000,
				last ? { label: 'Undo', callback: () => void undo(t, last.id) } : undefined
			);
			if (showFinished) withFinished = await roastedApi.stock(true);
			await onchange();
		} catch (e) {
			formError = e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);
			if (!open) notifications.add(`Could not record it: ${formError}`, 'error');
		} finally {
			busy = false;
		}
	}

	async function undo(t: Target, moveId: string) {
		try {
			await roastedApi.undoMove(t.id, moveId);
			if (showFinished) withFinished = await roastedApi.stock(true);
			await onchange();
			notifications.add(`Taken back: ${t.name}`, 'success');
		} catch (e) {
			notifications.add(`Could not take it back: ${e instanceof Error ? e.message : e}`, 'error');
		}
	}

	async function combine(t: Target) {
		if (!picked.length) {
			formError = 'Pick the roasts that went into the bag';
			return;
		}
		busy = true;
		try {
			const bag = await roastedApi.combine([t.id, ...picked], bagName.trim() || null);
			open = null;
			notifications.add(`In one bag: ${bag.name}, ${fmtGrams(bag.left_g)}`, 'success');
			if (showFinished) withFinished = await roastedApi.stock(true);
			await onchange();
		} catch (e) {
			formError = e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);
		} finally {
			busy = false;
		}
	}

	async function split(t: Target) {
		if (
			!confirm(
				`Split ${t.name} apart? Its roasts are separate again, each keeping its share of what was taken from the bag.`
			)
		)
			return;
		try {
			await roastedApi.uncombine(t.id);
			if (showFinished) withFinished = await roastedApi.stock(true);
			await onchange();
			notifications.add(`Split apart: ${t.name}`, 'success');
		} catch (e) {
			notifications.add(`Could not split it: ${e instanceof Error ? e.message : e}`, 'error');
		}
	}

	function submit(t: Target) {
		if (!open) return;
		if (open.kind === 'combine') {
			void combine(t);
			return;
		}
		if (open.kind === 'packed') {
			const b = parseBags(amount, weightState.unit);
			if (typeof b === 'string') {
				formError = b;
				return;
			}
			const req: RoastedMoveRequest = {
				kind: 'packed',
				...b,
				form: ground ? 'ground' : 'whole',
				note: note.trim() || null
			};
			if (ground && grind.trim()) req.grind = grind.trim();
			void record(t, req, 'Recorded the bags');
			return;
		}
		const a = parseAmount(amount, weightState.unit);
		if (typeof a === 'string') {
			formError = a;
			return;
		}
		const req: RoastedMoveRequest = { kind: open.kind, ...a, note: note.trim() || null };
		if (open.kind === 'given') req.recipient = to.trim() || null;
		void record(t, req, open.kind === 'given' ? 'Recorded the gift' : 'Recorded');
	}

	function finish(t: Target) {
		if (!confirm(`Mark ${t.name} as finished (${fmtGrams(t.left)} used)?`)) return;
		void record(t, { kind: 'used', all: true }, 'Finished');
	}

	const th = 'px-3 py-2 text-left text-xs font-medium text-muted-foreground';
	const link =
		'text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline disabled:opacity-40';
	const input = 'rounded-md border border-border bg-background px-2 py-1 text-sm';
</script>

{#snippet actions(r: LotRow)}
	{@const t = target(r)}
	<span class="flex flex-wrap gap-x-3 sm:justify-end">
		{#if isOn('labels')}<a
				class={link}
				href={resolve('/history/[id]/labels', { id: t.id })}
				title={t.bag ? 'Bag labels: every batch, the days roasted' : 'Bag labels for this roast'}
				>Labels</a
			>{/if}
		{#if isOn('reports') && !t.bag}<a
				class={link}
				href={resolve(`/reports?kind=roast&roast=${encodeURIComponent(t.id)}`)}
				title="This roast's report as a PDF">Report</a
			>{/if}
		{#if t.active}
			<button class={link} onclick={() => start(t, 'used')} disabled={busy}>Used…</button>
			<button class={link} onclick={() => start(t, 'given')} disabled={busy}>Gave…</button>
			<button
				class={link}
				onclick={() => start(t, 'packed')}
				disabled={busy}
				title="Bags packed from it: whole bean or ground">Packed…</button
			>
			<button class={link} onclick={() => finish(t)} disabled={busy}>Finished</button>
			{#if joinable(r).length}
				<button
					class={link}
					onclick={() => start(t, 'combine')}
					disabled={busy}
					data-testid="combine"
					title="Roasts that went into the same bag or jar: this coffee, or others for a blend"
					>{t.bag ? 'Add a roast…' : 'One bag with…'}</button
				>
			{/if}
		{/if}
		{#if t.bag}<button
				class={link}
				onclick={() => split(t)}
				disabled={busy}
				title="Its roasts separate again">Split</button
			>{/if}
	</span>
{/snippet}

{#snippet form(r: LotRow)}
	{@const t = target(r)}
	{#if open?.id === t.key && open.kind === 'combine'}
		<form
			class="flex flex-wrap items-end gap-2 pt-2"
			onsubmit={(e) => {
				e.preventDefault();
				submit(t);
			}}
		>
			<fieldset class="flex flex-col gap-1 text-xs text-muted-foreground">
				<legend class="pb-1">{t.bag ? 'Also in this bag' : 'In one bag with'}</legend>
				{#each joinable(r) as o (o.session_id)}
					<label class="flex items-center gap-2 text-sm text-foreground">
						<input type="checkbox" value={o.session_id} bind:group={picked} />
						{lotName(o)}
						<span class="text-xs text-muted-foreground"
							>{o.bean_id !== baseBean(r) && o.bean_name ? `${o.bean_name} · ` : ''}{fmtGrams(
								o.left_g
							)}</span
						>
					</label>
				{/each}
			</fieldset>
			{#if !t.bag}
				<label class="flex min-w-40 flex-1 flex-col text-xs text-muted-foreground">
					Name (optional)
					<input
						class={input}
						bind:value={bagName}
						placeholder={blendPicked(r)
							? 'the coffees and the batches'
							: 'the coffee and the batches'}
					/>
				</label>
			{/if}
			<button
				type="submit"
				class="rounded-md bg-action px-3 py-1 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				disabled={busy}>Save</button
			>
			<button
				type="button"
				class="rounded-md border border-border px-3 py-1 text-sm hover:bg-accent"
				onclick={() => (open = null)}>Cancel</button
			>
			<span class="w-full text-xs text-muted-foreground">
				{formError
					? ''
					: blendPicked(r)
						? "A blend: what is taken from it comes off each roast by its share, it is ready when the newest roast is, and its label names each coffee's origin with its share and prints every batch. Its tasting notes are yours to write on the label."
						: 'One bag: what is taken from it comes off each roast by its share, it is ready when the newest roast is, and its label prints every batch.'}
			</span>
			{#if formError}<span class="w-full text-xs text-red-400">{formError}</span>{/if}
		</form>
	{:else if open?.id === t.key}
		<form
			class="flex flex-wrap items-end gap-2 pt-2"
			onsubmit={(e) => {
				e.preventDefault();
				submit(t);
			}}
		>
			<label class="flex flex-col text-xs text-muted-foreground">
				{open.kind === 'given' ? 'Gave' : open.kind === 'packed' ? 'Bags' : 'Used'}
				<!-- svelte-ignore a11y_autofocus -->
				<input
					class="{input} w-28"
					bind:value={amount}
					placeholder={open.kind === 'packed'
						? weightState.unit === 'us'
							? '3x8oz'
							: '3x250g'
						: amountHint(weightState.unit)}
					autofocus
				/>
			</label>
			{#if open.kind === 'packed'}
				<label class="flex flex-col text-xs text-muted-foreground">
					As
					<select class={input} bind:value={ground} data-testid="pack-form">
						<option value={false}>Whole bean</option>
						<option value={true}>Ground</option>
					</select>
				</label>
				{#if ground}
					<label class="flex flex-col text-xs text-muted-foreground">
						Grind
						<input class="{input} w-32" bind:value={grind} list="grinds" placeholder="espresso" />
					</label>
					<datalist id="grinds"
						>{#each GRINDS as g (g)}<option value={g}></option>{/each}</datalist
					>
				{/if}
			{/if}
			{#if open.kind === 'given'}
				<label class="flex flex-col text-xs text-muted-foreground">
					To
					<input class="{input} w-32" bind:value={to} placeholder="Sam" />
				</label>
			{/if}
			<label class="flex min-w-32 flex-1 flex-col text-xs text-muted-foreground">
				Note
				<input class={input} bind:value={note} />
			</label>
			<button
				type="submit"
				class="rounded-md bg-action px-3 py-1 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				disabled={busy}>Save</button
			>
			<button
				type="button"
				class="rounded-md border border-border px-3 py-1 text-sm hover:bg-accent"
				onclick={() => (open = null)}>Cancel</button
			>
			<span class="w-full text-xs text-muted-foreground">
				{formError
					? ''
					: open.kind === 'packed'
						? 'Bags of a size, e.g. 3x250g; "3" is three of the bag size in Settings → Roasting. Ground keeps its best for less.'
						: `A dose is the ${t.for === 'filter' ? 'filter' : 'espresso'} dose and a bag the bag size in Settings → Roasting; "all" finishes it.${t.bag ? ' It comes off each roast in the bag by its share.' : ''}`}
			</span>
			{#if formError}<span class="w-full text-xs text-red-400">{formError}</span>{/if}
		</form>
	{/if}
{/snippet}

<section aria-labelledby="roasted-h" class="space-y-2">
	<div class="flex flex-wrap items-baseline gap-x-3">
		<h2 id="roasted-h" class="text-lg font-semibold text-foreground">Roasted coffee</h2>
		<span class="text-xs text-muted-foreground"
			>Ready from the roast date plus its rest; drink the oldest ready first.</span
		>
		<button class="ml-auto {link}" onclick={toggleFinished}
			>{showFinished ? 'Hide finished' : 'Show finished'}</button
		>
	</div>

	{#if groups.length}
		<ul class="space-y-0.5 text-sm">
			{#each groups as g (g.for)}
				<li class="text-foreground">
					{groupLine(g)}
					{#if g.low}<span class="ml-1 text-xs font-medium text-amber-400">RUNNING LOW</span>{/if}
				</li>
			{/each}
		</ul>
	{/if}

	{#if lots.length === 0}
		<p
			class="rounded-md border border-dashed border-border px-3 py-4 text-sm text-muted-foreground"
		>
			No roasted coffee yet. A finished roast with its roasted weight shows here, resting until it
			is ready.
		</p>
	{:else}
		<!-- Phone: cards -->
		<ul class="divide-y divide-border rounded-md border border-border bg-card sm:hidden">
			{#each rows as r (target(r).key)}
				<li class="px-3 py-2.5">
					{#if r.kind === 'lot'}
						{@const l = r.lot}
						<div class="flex items-baseline justify-between gap-2">
							<a
								href={resolve('/history/[id]', { id: l.session_id })}
								class="font-medium text-foreground hover:underline">{lotName(l)}</a
							>
							<span class="tabular-nums text-foreground"
								>{l.left_g != null ? fmtGrams(l.left_g) : '—'}</span
							>
						</div>
						<div class="text-xs {stateClass(l.state)}">{forLabel(l.for)} · {stateLine(l)}</div>
					{:else}
						<div class="flex items-baseline justify-between gap-2">
							<span class="font-medium text-foreground">{r.bag.name}</span>
							<span class="tabular-nums text-foreground">{fmtGrams(r.bag.left_g)}</span>
						</div>
						<div class="text-xs {stateClass(r.bag.state)}">
							{forLabel(r.members[0].for)} · {r.bag.blend ? 'a blend' : 'one bag'} · {bagStateLine(
								r.bag
							)}
						</div>
						{#if r.bag.blend}<div class="text-xs text-muted-foreground" data-testid="blend">
								{blendLine(r.bag)}
							</div>{/if}
						<div class="text-xs text-muted-foreground">
							{#each r.members as m, i (m.session_id)}{i ? ' · ' : ''}<a
									href={resolve('/history/[id]', { id: m.session_id })}
									class="hover:underline"
									>{m.batch_number != null ? `#${m.batch_number}` : m.name}</a
								>
								{fmtGrams(m.left_g)}{/each}
						</div>
					{/if}
					<div class="mt-1">{@render actions(r)}</div>
					{@render form(r)}
				</li>
			{/each}
		</ul>
		<!-- Wider: a table -->
		<div class="hidden overflow-x-auto rounded-md border border-border bg-card sm:block">
			<table class="w-full text-sm">
				<thead class="border-b border-border">
					<tr>
						<th class={th}>Roast</th>
						<th class={th}>For</th>
						<th class="{th} text-right">Left</th>
						<th class={th}>Rest</th>
						<th class={th}></th>
					</tr>
				</thead>
				<tbody class="divide-y divide-border">
					{#each rows as r (target(r).key)}
						<tr>
							{#if r.kind === 'lot'}
								{@const l = r.lot}
								<td class="px-3 py-2">
									<a
										href={resolve('/history/[id]', { id: l.session_id })}
										class="font-medium text-foreground hover:underline">{lotName(l)}</a
									>
									{#if l.bean_name}<div class="text-xs text-muted-foreground">
											{l.bean_name}
										</div>{/if}
								</td>
								<td class="px-3 py-2 text-xs text-muted-foreground">{forLabel(l.for)}</td>
								<td class="px-3 py-2 text-right tabular-nums whitespace-nowrap">
									<span class="text-foreground">{l.left_g != null ? fmtGrams(l.left_g) : '—'}</span>
									{#if l.roasted_g != null}<span class="text-xs text-muted-foreground">
											of {fmtGrams(l.roasted_g)}</span
										>{/if}
								</td>
								<td class="px-3 py-2 text-xs {stateClass(l.state)}">{stateLine(l)}</td>
							{:else}
								<td class="px-3 py-2" data-testid="bag">
									<span class="font-medium text-foreground">{r.bag.name}</span>
									{#if r.bag.blend}<div class="text-xs text-muted-foreground" data-testid="blend">
											Blend: {blendLine(r.bag)}
										</div>{/if}
									<div class="text-xs text-muted-foreground">
										One bag, roasted {bagDays(r.bag)}:
										{#each r.members as m, i (m.session_id)}{i ? ', ' : ''}<a
												href={resolve('/history/[id]', { id: m.session_id })}
												class="hover:underline"
												title="{fmtGrams(m.left_g)} of {fmtGrams(m.roasted_g)} left of this roast"
												>{m.batch_number != null ? `#${m.batch_number}` : m.name}</a
											>{/each}
									</div>
								</td>
								<td class="px-3 py-2 text-xs text-muted-foreground">{forLabel(r.members[0].for)}</td
								>
								<td class="px-3 py-2 text-right tabular-nums whitespace-nowrap">
									<span class="text-foreground">{fmtGrams(r.bag.left_g)}</span>
									<span class="text-xs text-muted-foreground"> of {fmtGrams(r.bag.roasted_g)}</span>
								</td>
								<td class="px-3 py-2 text-xs {stateClass(r.bag.state)}">{bagStateLine(r.bag)}</td>
							{/if}
							<td class="px-3 py-2 text-right">{@render actions(r)}</td>
						</tr>
						{#if open?.id === target(r).key}
							<tr><td colspan="5" class="px-3 pb-2">{@render form(r)}</td></tr>
						{/if}
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</section>
