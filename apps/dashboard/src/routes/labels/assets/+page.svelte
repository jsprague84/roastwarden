<script lang="ts">
	/** Label Studio → Assets: images and fonts a design uses by name (docs/labels.md, "Saved
	 *  designs, assets and brands"). Upload, look, copy the line a design needs, delete. */
	import { labelStudio, type Asset } from '#lib/api/labelStudio.js';
	import { labels as labelsApi } from '#lib/api/labels.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { assetName, typstFor } from '#lib/labels/studio.js';
	import { onMount } from 'svelte';
	import { Upload, Trash2, Copy, Download } from '@lucide/svelte';

	let assets = $state<Asset[]>([]);
	let builtinFonts = $state<string[]>([]);
	let loading = $state(true);
	let note = $state('');
	let busy = $state(false);
	let over = $state(false);
	let failures = $state<string[]>([]);
	let input: HTMLInputElement | undefined = $state();

	const images = $derived(assets.filter((a) => a.kind === 'image'));
	const fonts = $derived(assets.filter((a) => a.kind === 'font'));

	async function load() {
		try {
			const [a, o] = await Promise.all([labelStudio.assets(), labelsApi.options()]);
			assets = a;
			const own = new Set(a.map((x) => x.family).filter(Boolean));
			builtinFonts = o.fonts.filter((f) => !own.has(f));
		} catch (e) {
			notifications.add(`Assets: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			loading = false;
		}
	}
	onMount(load);

	// Each font asset is loaded into the page under its family, for its sample line
	// eslint-disable-next-line svelte/prefer-svelte-reactivity -- fonts already added to the page; the effect must not track it
	const loaded = new Set<string>();
	$effect(() => {
		for (const f of fonts) {
			if (!f.family || loaded.has(f.name)) continue;
			loaded.add(f.name);
			const face = new FontFace(`rr-asset-${f.name}`, `url(${labelStudio.assetUrl(f.name)})`);
			face
				.load()
				.then((ff) => document.fonts.add(ff))
				.catch(() => {});
		}
	});

	async function add(files: FileList | File[] | null) {
		if (!files?.length) return;
		busy = true;
		failures = [];
		let done = 0;
		for (const file of Array.from(files)) {
			const name = assetName(file.name);
			try {
				await labelStudio.uploadAsset(name, file, note);
				done++;
			} catch (e) {
				failures = [...failures, `${file.name}: ${e instanceof Error ? e.message : String(e)}`];
			}
		}
		busy = false;
		if (done) notifications.add(`Added ${done} asset${done === 1 ? '' : 's'}`, 'success');
		if (input) input.value = '';
		await load();
	}

	async function remove(a: Asset) {
		if (!confirm(`Delete ${a.name}? Designs that name it will fail to compile until it is back.`))
			return;
		try {
			await labelStudio.removeAsset(a.name);
			notifications.add(`Deleted ${a.name}`, 'success');
			await load();
		} catch (e) {
			notifications.add(`Delete: ${e instanceof Error ? e.message : String(e)}`, 'error');
		}
	}

	async function copy(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			notifications.add(`Copied: ${text}`, 'success');
		} catch {
			notifications.add(text, 'warning');
		}
	}

	const kb = (n: number) =>
		n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`;
</script>

<p class="max-w-3xl text-sm text-muted-foreground">
	Artwork, logos and fonts for your designs. A design names an image by its file name,
	<code class="text-foreground">image("logo.svg", width: 0.6in)</code>, and a font by its family,
	<code class="text-foreground">text(font: "Caveat")</code>. Images: SVG, PNG or JPEG up to 2 MB
	(SVG prints sharpest). Fonts: TTF or OTF up to 8 MB.
</p>

<div
	role="region"
	aria-label="Add assets"
	class="mt-4 flex flex-wrap items-center gap-3 rounded-md border border-dashed p-4 {over
		? 'border-focus bg-accent'
		: 'border-border'}"
	ondragover={(e) => {
		e.preventDefault();
		over = true;
	}}
	ondragleave={() => (over = false)}
	ondrop={(e) => {
		e.preventDefault();
		over = false;
		add(e.dataTransfer?.files ?? null);
	}}
>
	<Upload class="h-5 w-5 text-muted-foreground" />
	<span class="text-sm text-muted-foreground">Drop files here, or</span>
	<label
		class="cursor-pointer rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover"
	>
		{busy ? 'Adding…' : 'Choose files'}
		<input
			bind:this={input}
			type="file"
			multiple
			accept=".svg,.png,.jpg,.jpeg,.ttf,.otf"
			class="sr-only"
			disabled={busy}
			onchange={(e) => add(e.currentTarget.files)}
		/>
	</label>
	<input
		type="text"
		bind:value={note}
		placeholder="Where it came from, its licence (kept with it)"
		class="min-w-0 flex-1 rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground"
	/>
	<p class="w-full text-xs text-muted-foreground">
		A file with the name of one already there replaces it.
	</p>
	{#each failures as f (f)}<p class="w-full text-sm text-destructive">{f}</p>{/each}
</div>

{#if loading}
	<p class="mt-6 text-sm text-muted-foreground">Loading…</p>
{:else}
	<h2 class="mt-8 text-lg font-semibold text-foreground">
		Images <span class="text-sm font-normal text-muted-foreground">{images.length}</span>
	</h2>
	{#if images.length}
		<ul class="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
			{#each images as a (a.name)}
				<li class="min-w-0 rounded-md border border-border bg-card p-2">
					<div
						class="flex aspect-[4/3] items-center justify-center overflow-hidden rounded-sm bg-white p-2"
					>
						<img
							src={labelStudio.assetUrl(a.name)}
							alt={a.name}
							class="max-h-full max-w-full object-contain"
							loading="lazy"
						/>
					</div>
					<p class="mt-2 truncate font-mono text-xs text-foreground" title={a.name}>{a.name}</p>
					<p class="truncate text-xs text-muted-foreground" title={a.note ?? ''}>
						{kb(a.size)} · {a.created_by}{a.note ? ` · ${a.note}` : ''}
					</p>
					<div class="mt-1 flex gap-3 text-xs">
						<button
							class="inline-flex items-center gap-1 text-action hover:underline"
							onclick={() => copy(typstFor(a))}
							title="Copy the line a design uses"
						>
							<Copy class="h-3 w-3" /> Typst
						</button>
						<a
							class="inline-flex items-center gap-1 text-action hover:underline"
							href={labelStudio.assetUrl(a.name)}
							rel="external"
							download={a.name}
						>
							<Download class="h-3 w-3" /> File
						</a>
						<button
							class="ml-auto inline-flex items-center gap-1 text-muted-foreground hover:text-destructive"
							onclick={() => remove(a)}
							aria-label="Delete {a.name}"
						>
							<Trash2 class="h-3 w-3" />
						</button>
					</div>
				</li>
			{/each}
		</ul>
	{:else}
		<p class="mt-2 text-sm text-muted-foreground">
			None yet. A logo or a drawing in SVG is the best start.
		</p>
	{/if}

	<h2 class="mt-8 text-lg font-semibold text-foreground">
		Fonts <span class="text-sm font-normal text-muted-foreground">{fonts.length} added</span>
	</h2>
	{#if fonts.length}
		<ul class="mt-3 divide-y divide-border rounded-md border border-border">
			{#each fonts as a (a.name)}
				<li class="flex flex-wrap items-baseline gap-x-4 gap-y-1 px-3 py-2">
					<span class="w-48 truncate text-sm font-medium text-foreground" title={a.name}
						>{a.family ?? a.name}</span
					>
					<span
						class="min-w-0 flex-1 truncate text-xl text-foreground"
						style="font-family: 'rr-asset-{a.name}', serif">Guji natural · roasted 25 Oct</span
					>
					<span class="text-xs text-muted-foreground">{a.name} · {kb(a.size)}</span>
					<button class="text-xs text-action hover:underline" onclick={() => copy(typstFor(a))}
						>Copy Typst</button
					>
					<button
						class="text-muted-foreground hover:text-destructive"
						onclick={() => remove(a)}
						aria-label="Delete {a.name}"><Trash2 class="h-3.5 w-3.5" /></button
					>
				</li>
			{/each}
		</ul>
	{:else}
		<p class="mt-2 text-sm text-muted-foreground">
			None added. Google Fonts and Font Squirrel have fonts under the SIL Open Font License that you
			may embed.
		</p>
	{/if}
	<p class="mt-3 text-sm text-muted-foreground">
		Built in: {builtinFonts.join(', ')}.
	</p>
{/if}
