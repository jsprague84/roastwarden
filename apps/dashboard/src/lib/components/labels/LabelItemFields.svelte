<script lang="ts">
	/**
	 * One roast's labels (docs/labels.md, "Print runs"): the design and its settings, brand,
	 * copies, net weight, message, tasting notes and what shows. The same editor on a roast's
	 * Labels page and for each roast on Label Studio → Print; `compact` folds all but the
	 * design, copies, weight and message under "More".
	 */
	import { labelStudio, type Brand } from '#lib/api/labelStudio.js';
	import { resolve } from '$app/paths';
	import { paramSpecs, paramsFrom, startValues, type ParamSpec } from '#lib/labels/params.js';
	import { FIELD_LABELS, type LabelDesign } from '#lib/labels/labels.js';
	import { messageField, type RunItemAsk } from '#lib/labels/runs.js';
	import { GRINDS } from '#lib/beans/roasted.js';
	import ParamFields from './ParamFields.svelte';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';
	import { fmtGrams } from '#lib/beans/stock.js';
	import { weightUnit } from '#lib/stores/units.svelte.js';
	import { untrack } from 'svelte';

	type Own =
		| { name: string; label_design?: string | null; label_params?: Record<string, unknown> | null }
		| null
		| undefined;

	let {
		item = $bindable(),
		designs,
		brands = [],
		fields,
		sheet,
		own = null,
		roastedWeight = null,
		compact = false
	}: {
		item: RunItemAsk;
		designs: LabelDesign[];
		brands?: Brand[];
		/** The fields a label can leave off (`GET /api/labels`) */
		fields: string[];
		/** The sheet's id: the design is checked on it for its settings */
		sheet: string;
		/** The coffee's own label (its bean's design and settings) */
		own?: Own;
		roastedWeight?: number | null;
		compact?: boolean;
	} = $props();

	// The design the labels use: the one picked, else the coffee's own, else plain
	const effective = $derived(item.design || own?.label_design || 'plain');
	const designName = (id?: string | null) =>
		id ? (designs.find((d) => d.id === id.split('@')[0])?.name ?? id) : 'Plain';
	// A design not in the list (a pinned version, sam-gift@2) is still a choice
	const choices = $derived(
		!item.design || designs.some((d) => d.id === item.design)
			? designs
			: [{ id: item.design, name: item.design, kind: 'saved' as const }, ...designs]
	);

	// The design's settings from its manifest (a check on sample data reads it); what it
	// starts from: the coffee's own settings when it is the coffee's own design
	let specs = $state<ParamSpec[]>([]);
	let manifest = $state<unknown>(null);
	let values = $state<Record<string, unknown>>({});
	let base = $state<Record<string, unknown>>({});
	let shownFor = '';
	$effect(() => {
		const d = effective;
		const s = sheet;
		labelStudio
			.check({ design: d, sheet: s })
			.then((c) => {
				if (effective !== d) return;
				manifest = c.report?.manifest ?? null;
				specs = paramSpecs(manifest);
				const changed = untrack(() => shownFor) !== '' && untrack(() => shownFor) !== d;
				if (changed) item.params = {};
				shownFor = d;
				base = d === own?.label_design ? { ...(own?.label_params ?? {}) } : {};
				values = startValues(specs, { ...base, ...untrack(() => item.params ?? {}) });
			})
			.catch(() => {
				manifest = null;
				specs = [];
				values = {};
			});
	});
	// What the request carries; the server lays the coffee's own settings under it only when
	// no design is named
	$effect(() => {
		const p = paramsFrom(specs, values, item.design ? {} : base);
		if (JSON.stringify(p) !== JSON.stringify(untrack(() => item.params ?? {}))) item.params = p;
	});

	const message = $derived(messageField(manifest));
	// A design with its own recipient field: no message as well
	$effect(() => {
		if (!message.show && untrack(() => item.message)) item.message = '';
	});

	function toggle(f: string) {
		const hide = item.hide ?? [];
		item.hide = hide.includes(f) ? hide.filter((x) => x !== f) : [...hide, f];
	}
	const hiddenCount = $derived((item.hide ?? []).length);
	const extras = $derived(
		[
			Object.keys(item.params ?? {}).length ? 'settings' : '',
			item.brand ? 'brand' : '',
			item.notes?.trim() ? 'notes' : '',
			hiddenCount ? `${hiddenCount} off` : ''
		].filter(Boolean)
	);

	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const lbl = 'block text-xs font-medium text-muted-foreground';
</script>

{#snippet designPick()}
	<label class={lbl}
		>Design
		<select class={field} bind:value={item.design} data-testid="item-design">
			<option value=""
				>{own?.label_design
					? `Its own: ${designName(own.label_design)}`
					: 'Plain (default)'}</option
			>
			{#each choices as d (d.id)}<option value={d.id}
					>{d.name}{d.version ? ` (v${d.version})` : ''}</option
				>{/each}
		</select>
	</label>
{/snippet}

{#snippet copies()}
	<label class={lbl}
		>Copies
		<input class={field} type="number" min="1" max="200" bind:value={item.count} />
	</label>
{/snippet}

{#snippet weight()}
	<label class={lbl}
		>Net weight, {weightUnit()}
		<WeightInput
			class={field}
			bind:grams={item.weight_g}
			placeholder={roastedWeight ? `${fmtGrams(roastedWeight)} (roasted)` : 'roasted'}
		/>
	</label>
{/snippet}

{#snippet formPick()}
	<div class="grid grid-cols-2 gap-3">
		<label class={lbl}
			>In the bag
			<select class={field} bind:value={item.ground} data-testid="item-form">
				<option value={false}>Whole bean</option>
				<option value={true}>Ground</option>
			</select>
		</label>
		{#if item.ground}
			<label class={lbl}
				>Grind
				<input
					class={field}
					type="text"
					bind:value={item.grind}
					list="label-grinds"
					placeholder="espresso"
				/>
				<datalist id="label-grinds"
					>{#each GRINDS as g (g)}<option value={g}></option>{/each}</datalist
				>
			</label>
		{/if}
	</div>
{/snippet}

{#snippet messageInput()}
	{#if message.show}
		<label class={lbl} title={message.help}
			>{message.label}
			<input
				class={field}
				type="text"
				bind:value={item.message}
				placeholder={message.placeholder}
				data-testid="item-message"
			/>
		</label>
	{:else if message.help}
		<p class="text-xs text-muted-foreground">{message.help}</p>
	{/if}
{/snippet}

{#snippet more()}
	<ParamFields {specs} bind:values />
	{#if brands.length}
		<label class={lbl}
			>Brand
			<select class={field} bind:value={item.brand}>
				<option value="">Default (the coffee's, else Settings → Roasting)</option>
				{#each brands as b (b.id)}<option value={b.id}>{b.name}</option>{/each}
			</select>
		</label>
	{/if}
	<label class={lbl}
		>Tasting notes
		<input
			class={field}
			type="text"
			bind:value={item.notes}
			placeholder="the cupping's notes, else the coffee's"
		/>
	</label>
	<fieldset>
		<legend class={lbl}>On the label</legend>
		<div class="mt-1 grid grid-cols-2 gap-x-3 gap-y-1">
			{#each fields as f (f)}
				<label class="flex items-center gap-2 text-sm text-foreground">
					<input
						type="checkbox"
						checked={!(item.hide ?? []).includes(f)}
						onchange={() => toggle(f)}
					/>
					{FIELD_LABELS[f] ?? f}
				</label>
			{/each}
		</div>
	</fieldset>
{/snippet}

{#if compact}
	<div class="grid grid-cols-[minmax(0,1fr)_4.5rem_5.5rem] gap-2">
		{@render designPick()}
		{@render copies()}
		<label class={lbl}
			>Net {weightUnit()}
			<WeightInput
				class={field}
				bind:grams={item.weight_g}
				placeholder={roastedWeight ? fmtGrams(roastedWeight) : 'roasted'}
			/>
		</label>
	</div>
	<div class="mt-2">{@render messageInput()}</div>
	<details class="mt-2">
		<summary class="cursor-pointer text-xs text-muted-foreground hover:text-foreground">
			More: settings, brand, notes, what shows{extras.length ? ` (${extras.join(', ')})` : ''}
		</summary>
		<div class="mt-2 space-y-3">{@render formPick()}{@render more()}</div>
	</details>
{:else}
	<div class="space-y-3">
		{@render designPick()}
		<a
			href={resolve('/labels/designs/[id]', { id: encodeURIComponent(effective.split('@')[0]) })}
			class="-mt-2 block text-xs text-action hover:underline"
		>
			Open this design in Label Studio
		</a>
		<div class="grid grid-cols-2 gap-3">
			{@render copies()}
			{@render weight()}
		</div>
		{@render formPick()}
		{@render messageInput()}
		{@render more()}
	</div>
{/if}
