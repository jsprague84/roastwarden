<script lang="ts">
	/**
	 * Settings (docs/ui-structure.md): app-wide only, one page per group.
	 * Anything that belongs to a roaster is on that roaster's pages.
	 */
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { selectedDevice } from '#lib/stores/roast.svelte.js';
	import { ArrowUpRight } from '@lucide/svelte';
	import { SETTINGS, shown } from '#lib/modules/modules.js';
	import { modulesState } from '#lib/stores/modules.svelte.js';

	let { children } = $props();

	// Tokens, Approvals and Audit only with the agents module, under one heading (docs/lean-home.md)
	const SECTIONS = $derived(shown(SETTINGS, modulesState.on));
	const device = $derived(selectedDevice());
	const current = $derived(SECTIONS.find((s) => s.href === page.url.pathname) ?? SECTIONS[0]);
</script>

<svelte:head>
	<title>{current.label} · Settings | Roastwarden</title>
</svelte:head>

<div class="mx-auto max-w-5xl">
	<h1 class="text-2xl font-bold text-foreground">Settings</h1>

	<div class="mt-4 md:grid md:grid-cols-[11rem_minmax(0,1fr)] md:gap-8">
		<nav
			class="mb-5 flex flex-wrap border-b border-border md:mb-0 md:flex-col md:flex-nowrap md:self-start md:border-b-0 md:border-l"
			aria-label="Settings sections"
		>
			{#each SECTIONS as sec (sec.href)}
				{#if sec.group}
					<span
						class="hidden px-3 pb-1 pt-3 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground md:block"
						>{sec.group}</span
					>
				{/if}
				<!-- eslint-disable svelte/no-navigation-without-resolve -- a page from the SETTINGS table (lib/modules/modules.ts), typed as a route with no parameter; resolve() cannot take a union of routes -->
				<a
					href={sec.href}
					aria-current={current.href === sec.href ? 'page' : undefined}
					class="-mb-px whitespace-nowrap border-b-2 px-2 py-2 text-sm font-medium sm:px-3 md:-mb-0 md:-ml-px md:border-b-0 md:border-l-2
					{current.href === sec.href
						? 'border-action text-foreground'
						: 'border-transparent text-muted-foreground hover:text-foreground'}"
				>
					{sec.label}
				</a>
				<!-- eslint-enable svelte/no-navigation-without-resolve -->
			{/each}
			<a
				href={device ? resolve('/roasters/[id]/control', { id: device.id }) : resolve('/roasters')}
				class="hidden items-center gap-1 whitespace-nowrap px-3 py-2 text-sm text-muted-foreground hover:text-foreground md:mt-4 md:inline-flex"
				title={modulesState.on.includes('tuning')
					? 'PID, gain schedule, fan assist and autotune belong to a roaster'
					: 'The PID belongs to a roaster'}
			>
				Roaster control <ArrowUpRight class="h-3.5 w-3.5" />
			</a>
		</nav>

		<div class="min-w-0">
			{@render children()}
		</div>
	</div>
</div>
