<script lang="ts">
	/** Label Studio → Coffees: each coffee's own label (docs/labels.md, "A coffee's own
	 *  label"): the design, its settings and the brand its roasts' labels start with. */
	import { beans as beansApi, type Bean } from '#lib/api/beans.js';
	import { labelStudio, type Brand, type DesignItem } from '#lib/api/labelStudio.js';
	import { resolve } from '$app/paths';
	import { notifications } from '#lib/stores/notifications.js';
	import { changedParams, paramSpecs, startValues, type ParamSpec } from '#lib/labels/params.js';
	import ParamFields from '#lib/components/labels/ParamFields.svelte';
	import { onMount, untrack } from 'svelte';

	type WithLabel = Bean & {
		label_design?: string | null;
		label_params?: Record<string, unknown> | null;
		label_brand?: string | null;
	};

	let list = $state<WithLabel[]>([]);
	let designs = $state<DesignItem[]>([]);
	let brands = $state<Brand[]>([]);
	let loading = $state(true);
	let archived = $state(false);

	async function load() {
		try {
			[list, designs, brands] = await Promise.all([
				beansApi.list(archived) as Promise<WithLabel[]>,
				labelStudio.designs(),
				labelStudio.brands()
			]);
		} catch (e) {
			notifications.add(`Coffees: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			loading = false;
		}
	}
	onMount(load);

	const designName = (ref?: string | null) => {
		if (!ref) return '';
		const [id, v] = ref.split('@');
		const d = designs.find((x) => x.id === id);
		return `${d?.name ?? id}${v ? ` (version ${v}, pinned)` : ''}`;
	};
	const brandName = (id?: string | null) =>
		id ? (brands.find((b) => b.id === id)?.name ?? id) : '';
	const settingsText = (p?: Record<string, unknown> | null) =>
		p && Object.keys(p).length
			? Object.entries(p)
					.map(([k, v]) => `${k}=${String(v).split('\n')[0]}`)
					.join(', ')
			: '';

	// One coffee's label being changed
	let open = $state<WithLabel | null>(null);
	let design = $state('plain');
	let brand = $state('');
	let specs = $state<ParamSpec[]>([]);
	let values = $state<Record<string, unknown>>({});
	let preview = $state('');
	let saving = $state(false);

	function edit(b: WithLabel) {
		open = b;
		design = b.label_design ?? 'plain';
		brand = b.label_brand ?? '';
		preview = '';
	}

	// The design's settings, from its manifest; the coffee's own values when it is its design
	$effect(() => {
		const d = design;
		const b = open;
		if (!b) return;
		labelStudio
			.check({ design: d, sheet: '5163' })
			.then((c) => {
				if (design !== d) return;
				specs = paramSpecs(c.report?.manifest);
				values = startValues(specs, d === b.label_design ? (b.label_params ?? {}) : {});
			})
			.catch(() => {
				specs = [];
				values = {};
			});
	});
	const params = $derived(changedParams(specs, values));

	// A preview with the coffee's sample: its name and origin on sample roast data
	$effect(() => {
		const b = open;
		if (!b) return;
		// The coffee's own name and origin on sample roast data
		const ask = { design, sheet: '5163', params, brand: brand || undefined, bean: b.id, png: 130 };
		const t = setTimeout(async () => {
			try {
				const c = await labelStudio.check(ask);
				if (untrack(() => open?.id) === b.id)
					preview = c.png ? `data:image/png;base64,${c.png}` : '';
			} catch {
				preview = '';
			}
		}, 300);
		return () => clearTimeout(t);
	});

	async function save(clear = false) {
		if (!open) return;
		saving = true;
		try {
			await labelStudio.setBeanLabel(
				open.id,
				clear ? { design: null, params: {}, brand: null } : { design, params, brand: brand || null }
			);
			notifications.add(
				clear
					? `${open.name}: back to the default label`
					: `${open.name}: its labels use ${designName(design)}`,
				'success'
			);
			open = null;
			await load();
		} catch (e) {
			notifications.add(`Save: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			saving = false;
		}
	}

	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const lbl = 'block text-xs font-medium text-muted-foreground';
</script>

<div class="flex flex-wrap items-baseline gap-4">
	<p class="max-w-3xl text-sm text-muted-foreground">
		A coffee can carry its own label: its roasts' labels start with that design, its settings and
		brand, here, on a roast's Labels page and for <code class="text-foreground">roast labels</code>.
		The rest use the plain design and the default brand.
	</p>
	<label class="ml-auto flex items-center gap-2 text-sm text-muted-foreground">
		<input type="checkbox" bind:checked={archived} onchange={load} /> Used up too
	</label>
</div>

{#if loading}
	<p class="mt-6 text-sm text-muted-foreground">Loading…</p>
{:else if !list.length}
	<p class="mt-6 text-sm text-muted-foreground">
		No coffees yet: add one under <a class="text-action underline" href={resolve('/beans')}>Beans</a
		>.
	</p>
{:else}
	<table class="mt-5 w-full text-sm">
		<thead>
			<tr class="border-b border-border text-left text-xs text-muted-foreground">
				<th class="py-1.5 pr-3 font-medium">Coffee</th>
				<th class="py-1.5 pr-3 font-medium">Its label</th>
				<th class="hidden py-1.5 pr-3 font-medium md:table-cell">Settings</th>
				<th class="hidden py-1.5 pr-3 font-medium sm:table-cell">Brand</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each list as b (b.id)}
				<tr class="border-b border-border align-top">
					<td class="py-2 pr-3">
						<a
							href={resolve('/beans/[id]', { id: b.id })}
							class="font-medium text-foreground hover:underline">{b.name}</a
						>
						<p class="text-xs text-muted-foreground">
							{[b.origin, b.process].filter(Boolean).join(' · ')}
						</p>
					</td>
					<td class="py-2 pr-3">
						{#if b.label_design}
							<a
								class="text-foreground hover:underline"
								href={resolve('/labels/designs/[id]', {
									id: encodeURIComponent(b.label_design.split('@')[0])
								})}>{designName(b.label_design)}</a
							>
						{:else}
							<span class="text-muted-foreground">plain (default)</span>
						{/if}
					</td>
					<td class="hidden max-w-xs truncate py-2 pr-3 text-xs text-muted-foreground md:table-cell"
						>{settingsText(b.label_params)}</td
					>
					<td class="hidden py-2 pr-3 text-muted-foreground sm:table-cell"
						>{brandName(b.label_brand)}</td
					>
					<td class="py-2 text-right">
						<button class="text-action hover:underline" onclick={() => edit(b)}
							>{b.label_design ? 'Change' : 'Give it a label'}</button
						>
					</td>
				</tr>
				{#if open?.id === b.id}
					<tr class="border-b border-border">
						<td colspan="5" class="py-3">
							<div
								class="grid gap-4 rounded-md border border-border bg-card p-4 md:grid-cols-[18rem_minmax(0,1fr)]"
							>
								<div class="space-y-3">
									<label class={lbl}
										>Design
										<select bind:value={design} class={field}>
											{#if design.includes('@')}<option value={design}>{designName(design)}</option
												>{/if}
											{#each designs as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
										</select>
									</label>
									<ParamFields {specs} bind:values />
									<label class={lbl}
										>Brand
										<select bind:value={brand} class={field}>
											<option value="">The default</option>
											{#each brands as br (br.id)}<option value={br.id}>{br.name}</option>{/each}
										</select>
									</label>
									<div class="flex flex-wrap gap-2 pt-1">
										<button
											onclick={() => save()}
											disabled={saving}
											class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
										>
											{saving ? 'Saving…' : `Use for ${b.name}`}
										</button>
										{#if b.label_design}
											<button
												onclick={() => save(true)}
												disabled={saving}
												class="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent"
												>Back to the default</button
											>
										{/if}
										<button
											onclick={() => (open = null)}
											class="px-2 py-1.5 text-sm text-muted-foreground hover:text-foreground"
											>Cancel</button
										>
									</div>
								</div>
								<div class="min-w-0">
									<p class="text-xs text-muted-foreground">{b.name} on a sample roast, 4 × 2 in</p>
									<div
										class="mt-1 flex aspect-[2/1] max-w-lg items-center justify-center overflow-hidden rounded-sm bg-white shadow-sm"
									>
										{#if preview}<img
												src={preview}
												alt="The label"
												class="max-h-full max-w-full"
											/>{:else}<span class="text-xs text-neutral-400">…</span>{/if}
									</div>
								</div>
							</div>
						</td>
					</tr>
				{/if}
			{/each}
		</tbody>
	</table>
{/if}
