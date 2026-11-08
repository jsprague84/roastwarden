<script lang="ts">
	/** Label Studio → Designs: every design on sample data as the server lays it out (a PNG
	 *  from the check, one at a time), on the sheet shape picked. A card opens the design
	 *  page (source, live check, versions). */
	import { labelStudio, type DesignItem } from '#lib/api/labelStudio.js';
	import { resolve } from '$app/paths';
	import { notifications } from '#lib/stores/notifications.js';
	import { kept, keep } from '#lib/stores/kept.js';
	import { previewCache } from '#lib/labels/studio.js';
	import { onMount } from 'svelte';
	import { Plus } from '@lucide/svelte';

	const SHAPES = [
		{ id: '5163', name: '4 × 2 in', ratio: '2 / 1' },
		{ id: '6874', name: '3¾ × 3 in', ratio: '5 / 4' },
		{ id: '5294', name: '2½ in round', ratio: '1 / 1' }
	];
	const KINDS = [
		{ id: 'all', name: 'All' },
		{ id: 'saved', name: 'Saved' },
		{ id: 'builtin', name: 'Built in' },
		{ id: 'folder', name: 'Folder' }
	];
	const kindLabel: Record<string, string> = {
		saved: 'saved',
		folder: 'folder',
		builtin: 'built in'
	};

	let designs = $state<DesignItem[]>([]);
	let loading = $state(true);
	let sheet = $state(kept('labels.sheet', '5163'));
	let kind = $state(kept('labels.kind', 'all'));
	let find = $state('');
	// PNGs survive a trip to a design and back (a module-level cache), keyed by design@version
	let previews = $state<Record<string, string>>({ ...previewCache });

	$effect(() => keep('labels.sheet', sheet));
	$effect(() => keep('labels.kind', kind));

	const ref = (d: DesignItem) => (d.version ? `${d.id}@${d.version}` : d.id);
	const key = (d: DesignItem, s: string) => `${ref(d)}:${s}`;

	const shown = $derived(
		designs.filter(
			(d) =>
				(kind === 'all' || d.kind === kind) &&
				(!find.trim() ||
					`${d.id} ${d.name} ${d.description ?? ''}`
						.toLowerCase()
						.includes(find.trim().toLowerCase()))
		)
	);
	const counts = $derived(
		Object.fromEntries(
			KINDS.map((k) => [
				k.id,
				k.id === 'all' ? designs.length : designs.filter((d) => d.kind === k.id).length
			])
		)
	);

	onMount(async () => {
		try {
			designs = await labelStudio.designs();
		} catch (e) {
			notifications.add(`Labels: ${e instanceof Error ? e.message : String(e)}`, 'error');
		} finally {
			loading = false;
		}
	});

	// One render at a time (the server runs two at once); stops when the sheet changes
	$effect(() => {
		const want = sheet;
		const list = designs;
		let stop = false;
		(async () => {
			for (const d of list) {
				if (stop) return;
				const k = key(d, want);
				if (k in previewCache) continue;
				try {
					const c = await labelStudio.check({ design: ref(d), sheet: want, png: 110 });
					previewCache[k] = c.png ? `data:image/png;base64,${c.png}` : '';
				} catch {
					previewCache[k] = '';
				}
				if (!stop) previews = { ...previewCache };
			}
		})();
		return () => {
			stop = true;
		};
	});

	const ratio = $derived(SHAPES.find((s) => s.id === sheet)?.ratio ?? '2 / 1');

	function ago(iso?: string): string {
		if (!iso) return '';
		const days = Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000);
		return days <= 0
			? 'today'
			: days === 1
				? 'yesterday'
				: days < 31
					? `${days} days ago`
					: new Date(iso).toLocaleDateString();
	}
</script>

<div class="flex flex-wrap items-center gap-x-6 gap-y-3">
	<div class="flex gap-4 text-sm" role="tablist" aria-label="Label shape">
		{#each SHAPES as s (s.id)}
			<button
				role="tab"
				aria-selected={sheet === s.id}
				class="border-b-2 px-1 pb-1 {sheet === s.id
					? 'border-action text-foreground'
					: 'border-transparent text-muted-foreground hover:text-foreground'}"
				onclick={() => (sheet = s.id)}>{s.name}</button
			>
		{/each}
	</div>
	<div class="flex flex-wrap gap-1 text-sm" aria-label="Which designs">
		{#each KINDS as k (k.id)}
			{#if k.id === 'all' || counts[k.id]}
				<button
					class="rounded px-2 py-0.5 {kind === k.id
						? 'bg-selected text-selected-foreground'
						: 'text-muted-foreground hover:text-foreground'}"
					onclick={() => (kind = k.id)}
					>{k.name} <span class="tabular-nums opacity-70">{counts[k.id]}</span></button
				>
			{/if}
		{/each}
	</div>
	<input
		type="search"
		bind:value={find}
		placeholder="Find a design"
		class="w-44 rounded border border-border bg-input px-2 py-1 text-sm text-foreground"
	/>
	<a
		href={resolve('/labels/designs/[id]', { id: 'new' })}
		class="ml-auto inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover"
	>
		<Plus class="h-4 w-4" /> New design
	</a>
</div>

{#if loading}
	<p class="mt-6 text-sm text-muted-foreground">Loading…</p>
{:else if !shown.length}
	<p class="mt-6 text-sm text-muted-foreground">No design matches.</p>
{:else}
	<ul class="mt-5 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
		{#each shown as d (d.id)}
			<li class="min-w-0">
				<a
					href={resolve('/labels/designs/[id]', { id: encodeURIComponent(d.id) })}
					class="group block rounded-md border border-border bg-card p-3 hover:border-focus"
				>
					<div
						class="flex items-center justify-center overflow-hidden rounded-sm bg-white"
						style="aspect-ratio: {ratio}"
					>
						{#if previews[key(d, sheet)]}
							<img
								src={previews[key(d, sheet)]}
								alt="{d.name} on sample data"
								class="max-h-full max-w-full"
							/>
						{:else if key(d, sheet) in previews}
							<span class="text-xs text-neutral-500">no preview: open it to see why</span>
						{:else}
							<span class="text-xs text-neutral-400">…</span>
						{/if}
					</div>
					<div class="mt-2 flex items-baseline justify-between gap-2">
						<span class="truncate text-sm font-semibold text-foreground group-hover:underline"
							>{d.name}</span
						>
						<span class="shrink-0 text-xs text-muted-foreground">{kindLabel[d.kind] ?? d.kind}</span
						>
					</div>
					<p class="truncate font-mono text-xs text-muted-foreground">
						{ref(d)}{d.updated_at ? ` · ${ago(d.updated_at)}` : ''}
					</p>
					{#if d.description}
						<p class="mt-1 line-clamp-2 text-xs text-muted-foreground">{d.description}</p>
					{/if}
					{#if d.params?.length}
						<p class="mt-1 truncate text-xs text-muted-foreground">
							Settings: {d.params.join(', ')}
						</p>
					{/if}
					{#if d.used_by?.length}
						<p class="mt-1 truncate text-xs text-foreground">Used by {d.used_by.join(', ')}</p>
					{/if}
				</a>
			</li>
		{/each}
	</ul>
{/if}
