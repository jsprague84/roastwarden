<script lang="ts">
	import { resolve } from '$app/paths';
	import '../app.css';
	import { page } from '$app/state';
	import { onMount, onDestroy, untrack } from 'svelte';
	import {
		initTelemetrySocket,
		destroyTelemetrySocket,
		loadRorSettings,
		connectionStatus
	} from '#lib/stores/telemetry.js';
	import NotificationToast from '#lib/components/app/NotificationToast.svelte';
	import ApiKeyPrompt from '#lib/components/app/ApiKeyPrompt.svelte';
	import DemoChip from '#lib/components/app/DemoChip.svelte';
	import EmergencyStop from '#lib/components/app/EmergencyStop.svelte';
	import DeviceSelector from '#lib/components/app/DeviceSelector.svelte';
	import LiveRoastChip from '#lib/components/app/LiveRoastChip.svelte';
	import { unitState, setUnit } from '#lib/stores/units.svelte.js';
	import { initTheme, toggleTheme, isLight } from '#lib/stores/theme.svelte.js';
	import { initRoast } from '#lib/stores/roast.svelte.js';
	import { Menu, X, Sun, Moon } from '@lucide/svelte';
	import Mark from '#lib/components/ui/Mark.svelte';
	import { remember, sectionHref } from '#lib/nav/sections.js';
	import { kept, keep } from '#lib/stores/kept.js';
	import { goto } from '$app/navigation';
	import { NAV, moduleOfPath, shown } from '#lib/modules/modules.js';
	import { modulesState, loadModules } from '#lib/stores/modules.svelte.js';
	import { loadDemo } from '#lib/stores/demo.svelte.js';
	import ModuleOff from '#lib/components/app/ModuleOff.svelte';

	let { children } = $props();
	let menuOpen = $state(false);

	onMount(() => {
		initTheme();
		initTelemetrySocket();
		loadRorSettings();
		void loadDemo();
		// A new install opens its first-run page once, instead of the Roast page.
		void loadModules().then((m) => {
			if (m?.firstRun && page.url.pathname === '/')
				goto(resolve('/welcome'), { replaceState: true });
		});
		return initRoast();
	});

	onDestroy(() => {
		destroyTelemetrySocket();
	});

	const navItems = $derived(shown(NAV, modulesState.on));
	/** A page of an off module shows one line instead (docs/lean-home.md). */
	const offModule = $derived.by(() => {
		const m = moduleOfPath(page.url.pathname);
		return m && !modulesState.on.includes(m) ? m : null;
	});

	function isActive(href: string): boolean {
		if (href === '/') return page.url.pathname === '/';
		return page.url.pathname.startsWith(href);
	}

	const statusColor: Record<string, string> = {
		connected: 'bg-green-500',
		reconnecting: 'bg-yellow-500',
		disconnected: 'bg-red-500'
	};
	const statusText: Record<string, string> = {
		connected: 'Live data connected',
		reconnecting: 'Reconnecting to the server…',
		disconnected: 'Not connected to the server'
	};

	// Each section's link returns to the page last open in it (this tab only).
	let lastInSection = $state<Record<string, string>>(kept('sections', {}));
	$effect(() => {
		const next = remember(
			untrack(() => lastInSection),
			page.url.pathname,
			page.url.search
		);
		if (next !== lastInSection) {
			lastInSection = next;
			keep('sections', next);
		}
	});
	const hrefFor = (section: string) => sectionHref(lastInSection, section, page.url.pathname);

	// Close the phone menu on navigation.
	$effect(() => {
		void page.url.pathname;
		menuOpen = false;
	});
</script>

{#snippet unitToggle()}
	<div
		class="inline-flex overflow-hidden rounded-md border border-border text-xs"
		role="group"
		aria-label="Temperature unit"
	>
		{#each ['C', 'F'] as const as u (u)}
			<button
				type="button"
				class="px-2 py-1 font-medium transition-colors {unitState.unit === u
					? 'bg-highlight-soft text-highlight'
					: 'text-muted-foreground hover:bg-accent hover:text-foreground'}"
				aria-pressed={unitState.unit === u}
				onclick={() => setUnit(u)}
			>
				°{u}
			</button>
		{/each}
	</div>
{/snippet}

{#snippet themeToggle()}
	<button
		type="button"
		class="rounded-md border border-border p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
		title={isLight() ? 'Switch to dark theme' : 'Switch to light theme'}
		aria-label={isLight() ? 'Switch to dark theme' : 'Switch to light theme'}
		onclick={toggleTheme}
	>
		{#if isLight()}<Moon class="h-3.5 w-3.5" />{:else}<Sun class="h-3.5 w-3.5" />{/if}
	</button>
{/snippet}

<div class="flex min-h-screen flex-col bg-background">
	<header class="sticky top-0 z-30 border-b border-border bg-card">
		<div class="flex h-14 items-center gap-2 px-3 md:gap-4 md:px-4">
			<a href={resolve('/')} class="flex shrink-0 items-center gap-2" aria-label="Roastwarden home">
				<Mark class="h-5 w-5 text-foreground" />
				<span class="hidden text-base font-bold tracking-tight text-foreground sm:inline"
					>Roastwarden</span
				>
				<span
					class="inline-block h-2 w-2 rounded-full {statusColor[$connectionStatus] ??
						'bg-gray-400'}"
					title={statusText[$connectionStatus] ?? $connectionStatus}
					aria-label={statusText[$connectionStatus] ?? $connectionStatus}
				></span>
			</a>
			<DemoChip />

			<nav class="hidden h-full items-stretch lg:flex" aria-label="Sections">
				{#each navItems as item (item.href)}
					<!-- eslint-disable svelte/no-navigation-without-resolve -- the section's page from the NAV table, or the page last open in it (lib/nav/sections.ts): a pathname the router gave -->
					<a
						href={hrefFor(item.href)}
						title={item.hint}
						aria-current={isActive(item.href) ? 'page' : undefined}
						class="-mb-px flex items-center border-b-2 px-3 text-sm font-medium transition-colors
						{isActive(item.href)
							? 'border-action text-foreground'
							: 'border-transparent text-muted-foreground hover:text-foreground'}"
					>
						{item.label}
					</a>
					<!-- eslint-enable svelte/no-navigation-without-resolve -->
				{/each}
			</nav>

			<div class="ml-auto flex min-w-0 items-center gap-2">
				{#if page.url.pathname !== '/'}
					<div class="min-w-0"><LiveRoastChip /></div>
				{/if}
				<div class="hidden md:block"><DeviceSelector /></div>
				<div class="hidden items-center gap-2 lg:flex">
					{@render unitToggle()}
					{@render themeToggle()}
				</div>
				<EmergencyStop />
				<button
					class="rounded-md p-1.5 text-muted-foreground hover:bg-accent lg:hidden"
					onclick={() => (menuOpen = !menuOpen)}
					aria-label={menuOpen ? 'Close menu' : 'Open menu'}
					aria-expanded={menuOpen}
				>
					{#if menuOpen}<X class="h-6 w-6" />{:else}<Menu class="h-6 w-6" />{/if}
				</button>
			</div>
		</div>

		{#if menuOpen}
			<div class="border-t border-border px-3 pb-3 pt-2 lg:hidden">
				<nav class="grid grid-cols-1 gap-0.5 sm:grid-cols-2" aria-label="Sections">
					{#each navItems as item (item.href)}
						<!-- eslint-disable svelte/no-navigation-without-resolve -- the section's page from the NAV table, or the page last open in it (lib/nav/sections.ts): a pathname the router gave -->
						<a
							href={hrefFor(item.href)}
							aria-current={isActive(item.href) ? 'page' : undefined}
							class="flex items-center gap-3 border-l-2 px-3 py-2.5 text-sm font-medium
							{isActive(item.href)
								? 'border-action text-foreground'
								: 'border-transparent text-muted-foreground hover:text-foreground'}"
						>
							<span>{item.label}</span>
							<span class="ml-auto truncate text-xs font-normal text-muted-foreground"
								>{item.hint}</span
							>
						</a>
						<!-- eslint-enable svelte/no-navigation-without-resolve -->
					{/each}
				</nav>
				<div class="mt-3 flex flex-wrap items-center gap-3 border-t border-border pt-3">
					<span class="text-xs text-muted-foreground md:hidden">Roaster</span>
					<div class="md:hidden"><DeviceSelector compact /></div>
					<div class="ml-auto flex items-center gap-2">
						{@render unitToggle()}
						{@render themeToggle()}
					</div>
				</div>
			</div>
		{/if}
	</header>

	<main class="flex-1 p-3 md:p-4">
		{#if offModule}<ModuleOff module={offModule} />{:else}{@render children()}{/if}
	</main>

	<NotificationToast />
	<ApiKeyPrompt />
</div>
