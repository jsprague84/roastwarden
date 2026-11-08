<script lang="ts">
	/**
	 * Explanations in place (docs/ui-structure.md, "Explaining modes in place").
	 *
	 * - Block (default): the one-line explanation under a control, with a "?"
	 *   that opens the full text inline.
	 * - `inline`: only a "?" next to a label; it opens the line and the full
	 *   text in a popover, so dense panels stay one line per control.
	 */
	import type { Help } from '#lib/constants/help.js';

	let {
		help,
		class: cls = '',
		inline = false,
		links = []
	}: {
		help: Help;
		class?: string;
		inline?: boolean;
		links?: { title: string; url: string }[];
	} = $props();
	let open = $state(false);
	const id = `hint-${Math.random().toString(36).slice(2, 9)}`;

	let pop = $state<HTMLElement | null>(null);
	let btn = $state<HTMLElement | null>(null);

	// Place the popover under its button, kept inside the window.
	function place() {
		if (!pop || !btn) return;
		const r = btn.getBoundingClientRect();
		const w = Math.min(320, window.innerWidth - 16);
		pop.style.width = `${w}px`;
		const left = Math.max(8, Math.min(r.left - 12, window.innerWidth - w - 8));
		const below = r.bottom + 6;
		const h = pop.offsetHeight;
		const top = below + h > window.innerHeight - 8 ? Math.max(8, r.top - h - 6) : below;
		pop.style.left = `${left}px`;
		pop.style.top = `${top}px`;
	}

	function onToggle(e: Event) {
		open = (e as ToggleEvent).newState === 'open';
		if (open) place();
	}
</script>

{#snippet q(popover: boolean)}
	<button
		bind:this={btn}
		type="button"
		class="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full border align-[-2px] text-[10px] font-semibold leading-none hover:border-amber-500 hover:text-amber-400
		{open ? 'border-amber-500 text-amber-400' : 'border-border text-muted-foreground'}"
		aria-expanded={open}
		aria-controls={id}
		aria-label={open ? 'Hide explanation' : 'Explain'}
		title={popover ? help.line : open ? 'Hide explanation' : 'Explain'}
		popovertarget={popover ? id : undefined}
		onclick={popover ? undefined : () => (open = !open)}>?</button
	>
{/snippet}

{#if inline}
	{@render q(true)}
	<!-- Spans, so the popover can sit inside a label or a sentence. No display class: the browser hides it while closed. -->
	<span
		bind:this={pop}
		{id}
		popover="auto"
		ontoggle={onToggle}
		class="fixed inset-auto m-0 rounded-md border border-border bg-card p-3 text-left text-xs font-normal normal-case leading-snug tracking-normal text-foreground shadow-lg"
	>
		<span class="block font-medium">{help.line}</span>
		{#if help.more}<span class="mt-1.5 block text-muted-foreground">{help.more}</span>{/if}
		{#each links as l (l.url + l.title)}
			<a
				href={l.url}
				target="_blank"
				rel="external noopener noreferrer"
				class="mt-1 block text-[11px] text-muted-foreground underline decoration-dotted underline-offset-2 hover:text-foreground"
				>{l.title}</a
			>
		{/each}
	</span>
{:else}
	<div class="text-xs leading-snug text-muted-foreground {cls}">
		<p>
			{help.line}
			{#if help.more}{@render q(false)}{/if}
		</p>
		{#if open && help.more}
			<p
				{id}
				class="mt-1.5 border-l-2 border-amber-500/60 bg-muted/60 px-2.5 py-2 text-foreground/85"
			>
				{help.more}
			</p>
		{/if}
	</div>
{/if}
