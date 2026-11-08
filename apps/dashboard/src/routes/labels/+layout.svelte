<script lang="ts">
	/**
	 * Label Studio (docs/labels.md, docs/ui-structure.md): bag label designs, print runs and
	 * their history, the coffees that carry a label, artwork and fonts, brand kits. A section of its own;
	 * History → a roast → Labels… stays the quick path for one roast.
	 */
	import { resolve } from '$app/paths';
	import { page } from '$app/state';

	let { children } = $props();

	const TABS = [
		{
			href: resolve('/labels'),
			label: 'Designs',
			hint: 'Every design on sample data; open one to edit it'
		},
		{
			href: resolve('/labels/print'),
			label: 'Print',
			hint: 'One sheet of labels for one roast or several (a gift run)'
		},
		{
			href: resolve('/labels/history'),
			label: 'History',
			hint: 'What was printed when; PDFs, reprints, the next free label'
		},
		{
			href: resolve('/labels/printers'),
			label: 'Printers',
			hint: 'Where labels print: each printer, its tray, colour and calibration'
		},
		{
			href: resolve('/labels/coffees'),
			label: 'Coffees',
			hint: 'Each coffee’s own label: design, settings, brand'
		},
		{
			href: resolve('/labels/assets'),
			label: 'Assets',
			hint: 'Artwork, logos and fonts a design can use'
		},
		{
			href: resolve('/labels/brands'),
			label: 'Brands',
			hint: 'Your roaster’s name, place, logo, colours and fonts'
		}
	];

	function active(href: string, path: string): boolean {
		if (href === '/labels') return path === '/labels' || path.startsWith('/labels/designs');
		return path === href || path.startsWith(`${href}/`);
	}
	const current = $derived(TABS.find((t) => active(t.href, page.url.pathname)) ?? TABS[0]);
	// The design page uses the whole width: the editor and the preview side by side
	const wide = $derived(page.url.pathname.startsWith('/labels/designs/'));
</script>

<svelte:head>
	<title>{current.label} · Labels | Roastwarden</title>
</svelte:head>

<div class="mx-auto {wide ? 'max-w-[110rem]' : 'max-w-6xl'}">
	<div class="flex flex-wrap items-end gap-x-6 gap-y-2 border-b border-border">
		<h1 class="pb-2 text-2xl font-bold text-foreground">Label Studio</h1>
		<nav class="flex flex-wrap" aria-label="Label Studio">
			{#each TABS as t (t.href)}
				<a
					href={t.href}
					title={t.hint}
					aria-current={current.href === t.href ? 'page' : undefined}
					class="-mb-px whitespace-nowrap border-b-2 px-2 py-2 text-sm font-medium sm:px-3
					{current.href === t.href
						? 'border-action text-foreground'
						: 'border-transparent text-muted-foreground hover:text-foreground'}"
				>
					{t.label}
				</a>
			{/each}
		</nav>
	</div>
	<div class="mt-5 min-w-0">
		{@render children()}
	</div>
</div>
