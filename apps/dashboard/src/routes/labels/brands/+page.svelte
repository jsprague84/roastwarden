<script lang="ts">
	/** Label Studio → Brands: brand kits (docs/labels.md, "Saved designs, assets and brands"):
	 *  your roaster's name, tagline, place, website, logo, colours and fonts. A design reads
	 *  the kit as `l.brand`; the built-in "shop" design shows it, so the preview is that. */
	import { labelStudio, type Asset, type Brand } from '#lib/api/labelStudio.js';
	import { labels as labelsApi } from '#lib/api/labels.js';
	import { settings as settingsApi } from '#lib/api/settings.js';
	import { resolve } from '$app/paths';
	import { notifications } from '#lib/stores/notifications.js';
	import { idFrom, validId } from '#lib/labels/studio.js';
	import type { LabelBag } from '#lib/labels/labels.js';
	import { onMount } from 'svelte';
	import { Plus, Trash2 } from '@lucide/svelte';

	const EMPTY: Brand = {
		id: '',
		name: '',
		tagline: '',
		place: '',
		website: '',
		logo: '',
		ink: '',
		accent: '',
		paper: '',
		font_display: '',
		font_body: '',
		font_script: ''
	};

	let brands = $state<Brand[]>([]);
	let images = $state<Asset[]>([]);
	let fonts = $state<string[]>([]);
	let fallback = $state('');
	// Bags a sold label goes on (the retail check): the built-in ones and the owner's
	let bags = $state<LabelBag[]>([]);
	let ownBags = $state<LabelBag[]>([]);
	let defaultBag = $state('');
	let newBag = $state({ name: '', front_w: 6.75, front_h: 10.5 });
	let loading = $state(true);
	let editing = $state<Brand | null>(null);
	let isNew = $state(false);
	let saving = $state(false);
	let idTouched = $state(false);

	async function load() {
		try {
			const [b, a, o, s] = await Promise.all([
				labelStudio.brands(),
				labelStudio.assets(),
				labelsApi.options(),
				settingsApi.get()
			]);
			brands = b;
			images = a.filter((x) => x.kind === 'image');
			fonts = o.fonts;
			fallback = s.label_brand ?? '';
			bags = o.bags ?? [];
			defaultBag = o.bag ?? 'pouch-12oz';
			try {
				ownBags = s.label_bags ? (JSON.parse(s.label_bags) as LabelBag[]) : [];
			} catch {
				ownBags = [];
			}
		} catch (e) {
			notifications.add(`Brands: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			loading = false;
		}
	}
	onMount(load);

	function edit(b: Brand) {
		editing = {
			...EMPTY,
			...Object.fromEntries(Object.entries(b).map(([k, v]) => [k, v ?? '']))
		} as Brand;
		isNew = false;
	}
	function start() {
		editing = { ...EMPTY };
		isNew = true;
		idTouched = false;
	}

	// The kit as the API takes it: empty fields are not set
	function body(b: Brand): Partial<Brand> {
		const out: Record<string, string | null> = {};
		for (const [k, v] of Object.entries(b)) {
			if (k === 'id' || k === 'created_at' || k === 'updated_at') continue;
			const t = typeof v === 'string' ? v.trim() : v;
			out[k] = t ? (t as string) : null;
		}
		return out as Partial<Brand>;
	}

	$effect(() => {
		if (editing && isNew && !idTouched) editing.id = idFrom(editing.name);
	});

	async function save() {
		if (!editing) return;
		if (!editing.name.trim()) return notifications.add('A brand needs a name', 'error');
		if (!validId(editing.id))
			return notifications.add('Its id is letters, digits, - and _', 'error');
		saving = true;
		try {
			await labelStudio.putBrand(editing.id, body(editing));
			if (!fallback) {
				await settingsApi.set('label_brand', editing.id);
			}
			notifications.add(`Saved ${editing.name}`, 'success');
			editing = null;
			await load();
		} catch (e) {
			notifications.add(`Save: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			saving = false;
		}
	}

	async function makeDefault(b: Brand) {
		try {
			await settingsApi.set('label_brand', b.id);
			fallback = b.id;
			notifications.add(
				`${b.name} is on labels unless a coffee or a print says otherwise`,
				'success'
			);
		} catch (e) {
			notifications.add(`Default: ${e instanceof Error ? e.message : String(e)}`, 'error');
		}
	}

	async function remove(b: Brand) {
		if (!confirm(`Delete the brand ${b.name}?`)) return;
		try {
			await labelStudio.removeBrand(b.id);
			if (fallback === b.id) await settingsApi.set('label_brand', '');
			notifications.add(`Deleted ${b.name}`, 'success');
			await load();
		} catch (e) {
			notifications.add(`Delete: ${e instanceof Error ? e.message : String(e)}`, 'error');
		}
	}

	async function saveBags(list: LabelBag[]) {
		try {
			await settingsApi.set('label_bags', JSON.stringify(list));
			await load();
		} catch (e) {
			notifications.add(`Bags: ${e instanceof Error ? e.message : String(e)}`, 'error');
		}
	}
	async function addBag() {
		const id = idFrom(newBag.name);
		if (!id) return notifications.add('Give the bag a name', 'error');
		await saveBags([
			...ownBags.filter((b) => b.id !== id),
			{
				id,
				name: newBag.name.trim(),
				front_w: Number(newBag.front_w),
				front_h: Number(newBag.front_h)
			}
		]);
		newBag = { name: '', front_w: 6.75, front_h: 10.5 };
	}
	async function setDefaultBag(id: string) {
		try {
			await settingsApi.set('label_bag', id);
			defaultBag = id;
		} catch (e) {
			notifications.add(`Bags: ${e instanceof Error ? e.message : String(e)}`, 'error');
		}
	}
	// 21 CFR 101.7(i): the net weight's least letter height for a front of this area
	const minLetter = (area: number) =>
		area <= 5 ? '1/16' : area <= 25 ? '1/8' : area <= 100 ? '3/16' : area <= 400 ? '1/4' : '1/2';

	// The preview: the built-in "shop" design with the kit as it is in the form, a moment
	// after the last change
	let preview = $state('');
	let previewProblem = $state('');
	$effect(() => {
		const b = editing ? body(editing) : null;
		if (!b || !b.name) {
			preview = '';
			return;
		}
		const t = setTimeout(async () => {
			try {
				const c = await labelStudio.check({
					design: 'shop',
					sheet: '5163',
					brand_kit: b,
					png: 150
				});
				preview = c.png ? `data:image/png;base64,${c.png}` : '';
				previewProblem = c.errors?.length ? c.errors.map((x) => x.message).join('; ') : '';
			} catch (e) {
				previewProblem = e instanceof Error ? e.message : String(e);
			}
		}, 300);
		return () => clearTimeout(t);
	});

	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const lbl = 'block text-xs font-medium text-muted-foreground';
</script>

<div class="flex flex-wrap items-baseline gap-4">
	<p class="max-w-3xl text-sm text-muted-foreground">
		Your roaster's name, tagline, place, logo, colours and fonts. Designs that use a brand read it
		as
		<code class="text-foreground">l.brand</code>; the built-in <em>shop</em> design does. A coffee or
		a print can pick one; else the default is used.
	</p>
	{#if !editing}
		<button
			onclick={start}
			class="ml-auto inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover"
		>
			<Plus class="h-4 w-4" /> New brand
		</button>
	{/if}
</div>

{#if editing}
	<div
		class="mt-5 grid gap-6 rounded-md border border-border bg-card p-4 lg:grid-cols-[minmax(0,26rem)_minmax(0,1fr)]"
	>
		<form
			class="space-y-3"
			onsubmit={(e) => {
				e.preventDefault();
				save();
			}}
		>
			<h2 class="text-base font-semibold text-foreground">
				{isNew ? 'New brand' : `Edit ${editing.name}`}
			</h2>
			<div class="grid grid-cols-[minmax(0,1fr)_10rem] gap-3">
				<label class={lbl}
					>Name on the label
					<input
						class={field}
						bind:value={editing.name}
						required
						placeholder="Hill Street Roasting"
					/>
				</label>
				<label class={lbl}
					>Id
					<input
						class="{field} font-mono"
						bind:value={editing.id}
						disabled={!isNew}
						oninput={() => (idTouched = true)}
					/>
				</label>
			</div>
			<label class={lbl}
				>Tagline
				<input
					class={field}
					bind:value={editing.tagline}
					placeholder="Small batches, roasted at home"
				/>
			</label>
			<div class="grid grid-cols-2 gap-3">
				<label class={lbl}
					>Place (city, state, ZIP)
					<input class={field} bind:value={editing.place} placeholder="Portland, OR 97201" />
				</label>
				<label class={lbl}
					>Website
					<input class={field} bind:value={editing.website} placeholder="example.com" />
				</label>
			</div>
			<label class={lbl}
				>Logo
				<select class={field} bind:value={editing.logo}>
					<option value="">None</option>
					{#each images as a (a.name)}<option value={a.name}>{a.name}</option>{/each}
				</select>
				<span class="mt-1 block font-normal"
					>Add one under <a class="text-action underline" href={resolve('/labels/assets')}>Assets</a
					> (SVG prints sharpest).</span
				>
			</label>
			<fieldset class="grid grid-cols-3 gap-3">
				{#each [['ink', 'Ink'], ['accent', 'Accent'], ['paper', 'Paper']] as [k, name] (k)}
					<label class={lbl}
						>{name}
						<span class="mt-1 flex items-center gap-1">
							<input
								type="color"
								class="h-8 w-9 shrink-0 rounded border border-border bg-input {(
									editing as unknown as Record<string, string>
								)[k]
									? ''
									: 'opacity-30'}"
								title={(editing as unknown as Record<string, string>)[k]
									? undefined
									: 'Not set: the design’s own'}
								value={(editing as unknown as Record<string, string>)[k] || '#ffffff'}
								oninput={(e) =>
									((editing as unknown as Record<string, string>)[k] = e.currentTarget.value)}
							/>
							<input
								class="w-full min-w-0 rounded border border-border bg-input px-1.5 py-1.5 font-mono text-xs text-foreground"
								placeholder="not set"
								bind:value={(editing as unknown as Record<string, string>)[k]}
							/>
						</span>
					</label>
				{/each}
			</fieldset>
			<fieldset class="grid grid-cols-3 gap-3">
				{#each [['font_display', 'Display font'], ['font_body', 'Body font'], ['font_script', 'Script font']] as [k, name] (k)}
					<label class={lbl}
						>{name}
						<select class={field} bind:value={(editing as unknown as Record<string, string>)[k]}>
							<option value="">Design's own</option>
							{#each fonts as f (f)}<option value={f}>{f}</option>{/each}
						</select>
					</label>
				{/each}
			</fieldset>
			<div class="flex gap-2 pt-1">
				<button
					type="submit"
					disabled={saving}
					class="rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>
					{saving ? 'Saving…' : 'Save brand'}
				</button>
				<button
					type="button"
					onclick={() => (editing = null)}
					class="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent"
					>Cancel</button
				>
			</div>
		</form>
		<div class="min-w-0">
			<p class="text-xs font-medium text-muted-foreground">
				On a 4 × 2 in label, with the <em>shop</em> design and sample coffee
			</p>
			<div
				class="mt-2 flex aspect-[2/1] max-w-xl items-center justify-center overflow-hidden rounded-sm bg-white shadow-sm"
			>
				{#if preview}<img
						src={preview}
						alt="The brand on a label"
						class="max-h-full max-w-full"
					/>{:else}<span class="text-xs text-neutral-500">Give it a name to see it</span>{/if}
			</div>
			{#if previewProblem}<p class="mt-2 text-sm text-destructive">{previewProblem}</p>{/if}
		</div>
	</div>
{/if}

{#if loading}
	<p class="mt-6 text-sm text-muted-foreground">Loading…</p>
{:else if brands.length}
	<ul class="mt-5 grid gap-3 sm:grid-cols-2">
		{#each brands as b (b.id)}
			<li class="flex min-w-0 gap-3 rounded-md border border-border bg-card p-3">
				<div
					class="flex h-14 w-14 shrink-0 items-center justify-center overflow-hidden rounded-sm bg-white"
				>
					{#if b.logo}<img
							src={labelStudio.assetUrl(b.logo)}
							alt=""
							class="max-h-full max-w-full"
						/>{/if}
				</div>
				<div class="min-w-0 flex-1">
					<p class="truncate text-sm font-semibold text-foreground">
						{b.name}
						{#if fallback === b.id}<span class="ml-1 text-xs font-normal text-highlight"
								>default</span
							>{/if}
					</p>
					<p class="truncate text-xs text-muted-foreground">
						{[b.tagline, b.place, b.website].filter(Boolean).join(' · ') || b.id}
					</p>
					<div class="mt-1 flex items-center gap-1">
						{#each [b.ink, b.accent, b.paper].filter(Boolean) as c (c)}
							<span
								class="h-3.5 w-3.5 rounded-sm border border-border"
								style="background: {c}"
								title={c}
							></span>
						{/each}
						<span class="ml-1 truncate text-xs text-muted-foreground"
							>{[b.font_display, b.font_body, b.font_script].filter(Boolean).join(', ')}</span
						>
					</div>
					<div class="mt-1.5 flex gap-3 text-xs">
						<button class="text-action hover:underline" onclick={() => edit(b)}>Edit</button>
						{#if fallback !== b.id}<button
								class="text-action hover:underline"
								onclick={() => makeDefault(b)}>Make default</button
							>{/if}
						<button
							class="ml-auto text-muted-foreground hover:text-destructive"
							onclick={() => remove(b)}
							aria-label="Delete {b.name}"><Trash2 class="h-3.5 w-3.5" /></button
						>
					</div>
				</div>
			</li>
		{/each}
	</ul>
{:else if !editing}
	<p class="mt-6 text-sm text-muted-foreground">
		No brand yet. One is worth it if you gift or sell coffee under a name.
	</p>
{/if}

<section class="mt-10">
	<h2 class="text-lg font-semibold text-foreground">Bags, for coffee that is sold</h2>
	<p class="mt-1 max-w-3xl text-sm text-muted-foreground">
		The retail check sizes the net weight by the bag's front, not the label: the label goes on a
		bag, and the rules look at the bag. The default is used when a check says only "for sale".
	</p>
	<table class="mt-3 w-full max-w-3xl text-sm">
		<thead
			><tr class="border-b border-border text-left text-xs text-muted-foreground"
				><th class="py-1 font-medium">Bag</th><th class="font-medium">Front</th><th
					class="font-medium">Net weight letters</th
				><th></th></tr
			></thead
		>
		<tbody>
			{#each bags as b (b.id)}
				<tr class="border-b border-border">
					<td class="py-1.5 text-foreground"
						>{b.name} <span class="font-mono text-xs text-muted-foreground">{b.id}</span></td
					>
					<td class="tabular-nums text-muted-foreground"
						>{b.front_w} × {b.front_h} in ({(b.front_w * b.front_h).toFixed(1)} sq in)</td
					>
					<td class="tabular-nums text-muted-foreground">{minLetter(b.front_w * b.front_h)} in</td>
					<td class="whitespace-nowrap text-right text-xs">
						{#if defaultBag === b.id}<span class="text-highlight">default</span>{:else}<button
								class="text-action hover:underline"
								onclick={() => setDefaultBag(b.id)}>Make default</button
							>{/if}
						{#if ownBags.some((o) => o.id === b.id)}· <button
								class="text-muted-foreground hover:text-destructive"
								onclick={() => saveBags(ownBags.filter((o) => o.id !== b.id))}>Remove</button
							>{/if}
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
	<form
		class="mt-3 flex max-w-3xl flex-wrap items-end gap-3"
		onsubmit={(e) => {
			e.preventDefault();
			addBag();
		}}
	>
		<label class="min-w-48 flex-1 {lbl}"
			>Your bag
			<input class={field} bind:value={newBag.name} placeholder="12 oz kraft valve bag" />
		</label>
		<label class="w-28 {lbl}"
			>Front width, in
			<input class={field} type="number" step="0.125" min="1" bind:value={newBag.front_w} />
		</label>
		<label class="w-28 {lbl}"
			>height, in
			<input class={field} type="number" step="0.125" min="1" bind:value={newBag.front_h} />
		</label>
		<button
			type="submit"
			class="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent"
			>Add the bag</button
		>
	</form>
</section>
