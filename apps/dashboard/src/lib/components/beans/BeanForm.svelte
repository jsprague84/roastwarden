<script lang="ts">
	import Hint from '#lib/components/ui/Hint.svelte';
	/**
	 * Add or edit a green coffee lot (docs/batches.md), with what the supplier publishes under
	 * "Supplier details" (docs/green-coffee.md). Repeated fields suggest earlier entries and
	 * seeded lists (GET /api/beans/suggestions); any value can still be typed.
	 */
	import {
		beans as beansApi,
		type Bean,
		type BeanRequest,
		type PickLists
	} from '#lib/api/beans.js';
	import { numOrNull } from '#lib/roast/properties.js';
	import { weightUnit } from '#lib/stores/units.svelte.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';

	let {
		bean = null,
		busy = false,
		onsave,
		oncancel
	}: {
		/** Editing this bean; null adds one (with a first stock). */
		bean?: Bean | null;
		busy?: boolean;
		onsave: (req: BeanRequest) => void;
		oncancel: () => void;
	} = $props();

	// svelte-ignore state_referenced_locally
	const b = bean;
	let name = $state(b?.name ?? '');
	let origin = $state(b?.origin ?? '');
	let variety = $state(b?.variety ?? '');
	let process = $state(b?.process ?? '');
	let lot = $state(b?.lot ?? '');
	let supplier = $state(b?.supplier ?? '');
	let notes = $state(b?.notes ?? '');
	// What its labels print
	let labelNotes = $state(b?.label_notes ?? '');
	let shortName = $state(b?.short_name ?? '');
	const suggested = b?.label_notes_suggested ?? null;
	// Number inputs bind numbers (or null once cleared).
	let moisture = $state<number | string | null>(b?.moisture_pct ?? '');
	let density = $state<number | string | null>(b?.density_g_l ?? '');
	let low = $state<number | null>(b?.low_stock_g ?? 500);
	let stock = $state<number | null>(null);
	// Supplier details
	let region = $state(b?.region ?? '');
	let producer = $state(b?.producer ?? '');
	let altitude = $state(b?.altitude ?? '');
	let grade = $state(b?.grade ?? '');
	let screen = $state(b?.screen_size ?? '');
	let certifications = $state(b?.certifications ?? '');
	let drying = $state(b?.drying ?? '');
	let score = $state<number | string | null>(b?.score ?? '');
	let roastRec = $state(b?.roast_recommendation ?? '');
	let cupping = $state(b?.cupping_notes ?? '');
	let url = $state(b?.url ?? '');
	let sku = $state(b?.sku ?? '');
	let showDetails = $state(
		!!(b?.region || b?.producer || b?.altitude || b?.grade || b?.url || b?.cupping_notes)
	);

	let picks = $state<PickLists | null>(null);
	$effect(() => {
		beansApi
			.suggestions()
			.then((p) => (picks = p))
			.catch(() => {});
	});
	const uid = $props.id();
	const urlBad = $derived(!!url.trim() && !/^https?:\/\//i.test(url.trim()));

	function save(e: SubmitEvent) {
		e.preventDefault();
		if (urlBad) return;
		const text = (v: string) => v.trim() || null;
		const req: BeanRequest = {
			name: name.trim(),
			origin: text(origin),
			variety: text(variety),
			process: text(process),
			lot: text(lot),
			supplier: text(supplier),
			notes: text(notes),
			label_notes: text(labelNotes),
			short_name: text(shortName),
			moisture_pct: numOrNull(moisture),
			density_g_l: numOrNull(density),
			low_stock_g: low ?? 500,
			region: text(region),
			producer: text(producer),
			altitude: text(altitude),
			grade: text(grade),
			screen_size: text(screen),
			certifications: text(certifications),
			drying: text(drying),
			score: numOrNull(score),
			roast_recommendation: text(roastRec),
			cupping_notes: text(cupping),
			url: text(url),
			sku: text(sku)
		};
		if (!b) {
			if (stock != null && stock > 0) req.stock_g = stock;
		}
		onsave(req);
	}

	const field =
		'w-full rounded-md border border-border bg-input px-2.5 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const label = 'mb-0.5 block text-xs font-medium text-muted-foreground';
</script>

{#snippet list(id: string, values: string[] | undefined)}
	<datalist id="{uid}-{id}">
		{#each values ?? [] as v (v)}<option value={v}></option>{/each}
	</datalist>
{/snippet}

<form onsubmit={save} class="grid grid-cols-2 gap-x-3 gap-y-2 sm:grid-cols-4">
	<div class="col-span-2">
		<label for="bean-name" class={label}>Name</label>
		<input
			id="bean-name"
			bind:value={name}
			required
			placeholder="Brazil Cerrado natural"
			class={field}
		/>
	</div>
	<div>
		<label for="bean-origin" class={label}>Origin</label>
		<input
			id="bean-origin"
			bind:value={origin}
			placeholder="Brazil"
			list="{uid}-origin"
			class={field}
		/>
	</div>
	<div>
		<label for="bean-variety" class={label}>Variety</label>
		<input
			id="bean-variety"
			bind:value={variety}
			placeholder="Catuai"
			list="{uid}-variety"
			class={field}
		/>
	</div>
	<div>
		<label for="bean-process" class={label}>Process</label>
		<input
			id="bean-process"
			bind:value={process}
			placeholder="Natural"
			list="{uid}-process"
			class={field}
		/>
	</div>
	<div>
		<label for="bean-lot" class={label}>Lot</label>
		<input id="bean-lot" bind:value={lot} class={field} />
	</div>
	<div class="col-span-2">
		<label for="bean-supplier" class={label}>Supplier</label>
		<input id="bean-supplier" bind:value={supplier} list="{uid}-supplier" class={field} />
	</div>
	{#if !b}
		<div>
			<label for="bean-stock" class={label}>In stock ({weightUnit()})</label>
			<WeightInput id="bean-stock" bind:grams={stock} placeholder="0" class={field} />
		</div>
	{/if}
	<div>
		<label for="bean-low" class={label} title="The bean shows as low below this"
			>Low below ({weightUnit()})</label
		>
		<WeightInput id="bean-low" bind:grams={low} class={field} />
	</div>
	<div>
		<label for="bean-moisture" class={label} title="Copied to each roast of this bean"
			>Moisture (%)</label
		>
		<input
			id="bean-moisture"
			type="number"
			min="0"
			max="30"
			step="0.1"
			bind:value={moisture}
			class={field}
		/>
	</div>
	<div>
		<label for="bean-density" class={label} title="Copied to each roast of this bean"
			>Density (g/L)</label
		>
		<input id="bean-density" type="number" min="0" step="1" bind:value={density} class={field} />
	</div>
	<div class="col-span-2 sm:col-span-4">
		<label for="bean-notes" class={label}>Notes</label>
		<input id="bean-notes" bind:value={notes} class={field} />
	</div>
	<div class="col-span-2 sm:col-span-3">
		<label for="bean-label-notes" class="{label} flex items-center gap-1"
			>Label notes <Hint
				inline
				help={{
					line: 'A few tasting words its bag labels print, e.g. "chocolate · coconut · apple".',
					more: "Without them a label prints the roast's cupping notes, else the supplier's notes cut at a sentence. At most 120 characters."
				}}
			/></label
		>
		<input
			id="bean-label-notes"
			bind:value={labelNotes}
			maxlength="120"
			placeholder={suggested ?? 'chocolate · coconut · apple'}
			class={field}
		/>
		{#if suggested && !labelNotes.trim()}
			<button
				type="button"
				class="mt-0.5 text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
				onclick={() => (labelNotes = suggested)}
				>Use “{suggested}” (from the supplier's notes)</button
			>
		{/if}
	</div>
	<div>
		<label for="bean-short-name" class="{label} flex items-center gap-1"
			>Label name <Hint
				inline
				help={{
					line: 'The name its labels print when the bean\'s own is long, e.g. "Kenya Gatugi".',
					more: "Empty: the bean's name. At most 40 characters."
				}}
			/></label
		>
		<input
			id="bean-short-name"
			bind:value={shortName}
			maxlength="40"
			placeholder={name.trim() || 'the name'}
			class={field}
		/>
	</div>

	<div class="col-span-2 sm:col-span-4">
		<button
			type="button"
			class="text-xs text-muted-foreground underline underline-offset-2 hover:text-foreground"
			aria-expanded={showDetails}
			onclick={() => (showDetails = !showDetails)}
		>
			{showDetails
				? 'Hide supplier details'
				: 'Supplier details: region, producer, altitude, grade, score, link…'}
		</button>
	</div>
	{#if showDetails}
		<div>
			<label for="bean-region" class={label}>Region</label>
			<input id="bean-region" bind:value={region} list="{uid}-region" class={field} />
		</div>
		<div>
			<label for="bean-producer" class={label}>Farm or producer</label>
			<input id="bean-producer" bind:value={producer} class={field} />
		</div>
		<div>
			<label for="bean-altitude" class={label}>Altitude</label>
			<input id="bean-altitude" bind:value={altitude} placeholder="1900–2200 masl" class={field} />
		</div>
		<div>
			<label for="bean-drying" class={label}>Drying</label>
			<input id="bean-drying" bind:value={drying} list="{uid}-drying" class={field} />
		</div>
		<div>
			<label for="bean-grade" class={label}>Grade</label>
			<input id="bean-grade" bind:value={grade} placeholder="Grade 1, AA, SHB" class={field} />
		</div>
		<div>
			<label for="bean-screen" class="{label} flex items-center gap-1"
				>Screen size <Hint
					inline
					help={{
						line: 'Sweet Maria\'s ".6 d/300gr, 15-17 Screen" is a defect count per 300 g (grade) and the screen size, not density.',
						more: 'Density is its own field: grams per litre, free-settled (a jug and a scale). Nothing reads a density from this one.'
					}}
				/></label
			>
			<input id="bean-screen" bind:value={screen} placeholder="16-18" class={field} />
		</div>
		<div>
			<label for="bean-score" class={label}>Supplier score</label>
			<input
				id="bean-score"
				type="number"
				min="0"
				max="100"
				step="0.1"
				bind:value={score}
				class={field}
			/>
		</div>
		<div>
			<label for="bean-certs" class={label}>Certifications</label>
			<input
				id="bean-certs"
				bind:value={certifications}
				list="{uid}-certifications"
				placeholder="Organic, Fairtrade"
				class={field}
			/>
		</div>
		<div class="col-span-2">
			<label for="bean-roastrec" class={label}>Roast recommendation</label>
			<input id="bean-roastrec" bind:value={roastRec} placeholder="City to City+" class={field} />
		</div>
		<div class="col-span-2">
			<label for="bean-cupping" class={label}>Cupping notes</label>
			<input id="bean-cupping" bind:value={cupping} class={field} />
		</div>
		<div class="col-span-2 sm:col-span-3">
			<label for="bean-url" class={label}>Link to the offer</label>
			<input
				id="bean-url"
				type="url"
				bind:value={url}
				placeholder="https://"
				class="{field} {urlBad ? 'border-red-400' : ''}"
			/>
		</div>
		<div>
			<label for="bean-sku" class={label}>SKU</label>
			<input id="bean-sku" bind:value={sku} class={field} />
		</div>
	{/if}

	<div class="col-span-2 flex gap-2 sm:col-span-4">
		<button
			type="submit"
			disabled={busy || !name.trim() || urlBad}
			class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
		>
			{busy ? 'Saving…' : b ? 'Save' : 'Add bean'}
		</button>
		<button
			type="button"
			onclick={oncancel}
			class="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent"
			>Cancel</button
		>
	</div>
	{@render list('origin', picks?.origin)}
	{@render list('variety', picks?.variety)}
	{@render list('process', picks?.process)}
	{@render list('supplier', picks?.supplier)}
	{@render list('region', picks?.region)}
	{@render list('drying', picks?.drying)}
	{@render list('certifications', picks?.certifications)}
</form>
