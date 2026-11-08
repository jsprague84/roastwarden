<script lang="ts">
	/**
	 * A card that folds to its header. The header carries a one-line summary,
	 * so a folded panel still says what it is doing. Fold state is kept per
	 * browser under `key`.
	 */
	import type { Snippet } from 'svelte';
	import { untrack, getContext } from 'svelte';
	import { FLAT_PANELS } from './flat.js';
	import { ChevronDown } from '@lucide/svelte';

	let {
		title,
		key,
		open: initial = true,
		icon,
		summary,
		actions,
		children,
		class: cls = ''
	}: {
		title: string;
		key?: string;
		open?: boolean;
		icon?: Snippet;
		summary?: Snippet;
		actions?: Snippet;
		children: Snippet;
		class?: string;
	} = $props();

	const storageKey = $derived(key ? `roastwarden.panel.${key}` : null);
	function load(): boolean {
		if (!storageKey) return initial;
		try {
			const v = localStorage.getItem(storageKey);
			return v == null ? initial : v === '1';
		} catch {
			return initial;
		}
	}
	// Inside a tab body (FlatPanels) the tab is the header: no card, no fold.
	const flat = getContext<boolean>(FLAT_PANELS) === true;

	// Read once: the fold state then belongs to the viewer.
	let open = $state(untrack(load));
	function toggle() {
		open = !open;
		if (!storageKey) return;
		try {
			localStorage.setItem(storageKey, open ? '1' : '0');
		} catch {
			/* storage unavailable */
		}
	}
</script>

{#if flat}
	<section class={cls} aria-label={title}>
		{#if actions}<div class="mb-2 flex items-center justify-end gap-1.5">
				{@render actions()}
			</div>{/if}
		{@render children()}
	</section>
{:else}
	<section class="rounded-lg border border-border bg-card {cls}">
		<header class="flex items-center gap-2 px-4 py-3">
			<button
				type="button"
				class="flex min-w-0 flex-1 items-center gap-2 text-left"
				aria-expanded={open}
				onclick={toggle}
			>
				{#if icon}<span class="shrink-0 text-muted-foreground">{@render icon()}</span>{/if}
				<span class="shrink-0 text-sm font-semibold text-foreground">{title}</span>
				{#if summary}<span class="min-w-0 flex-1 truncate text-xs text-muted-foreground"
						>{@render summary()}</span
					>{:else}<span class="flex-1"></span>{/if}
				<ChevronDown
					class="h-4 w-4 shrink-0 text-muted-foreground transition-transform {open
						? 'rotate-180'
						: ''}"
				/>
			</button>
			{#if actions}<div class="flex shrink-0 items-center gap-1.5">{@render actions()}</div>{/if}
		</header>
		{#if open}
			<div class="border-t border-border px-4 pb-4 pt-3">
				{@render children()}
			</div>
		{/if}
	</section>
{/if}
