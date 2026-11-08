<script lang="ts">
	// On a public demo (ROASTWARDEN_DEMO=1, docs/DEPLOY.md "A public demo"): "Demo" in the top
	// bar opens a small popover: what the demo is, where to install your own, how to send
	// feedback, the discussions and the running release's source (About's, AGPL section 13).
	// Nothing shows on any other server.
	import { page } from '$app/state';
	import { about } from '#lib/api/about.js';
	import { demoState } from '#lib/stores/demo.svelte.js';
	import {
		DISCUSSIONS_URL,
		FEEDBACK_EMAIL,
		INSTALL_URL,
		SITE_URL,
		feedbackHref
	} from '#lib/demo/demo.js';

	let sourceUrl = $state<string | null>(null);

	const id = 'demo-about';
	let btn = $state<HTMLElement | null>(null);
	let pop = $state<HTMLElement | null>(null);
	let open = $state(false);

	// Under the chip, kept inside the window (as Hint's popover).
	function place() {
		if (!pop || !btn) return;
		const r = btn.getBoundingClientRect();
		const w = Math.min(300, window.innerWidth - 16);
		pop.style.width = `${w}px`;
		pop.style.left = `${Math.max(8, Math.min(r.left, window.innerWidth - w - 8))}px`;
		pop.style.top = `${r.bottom + 6}px`;
	}
</script>

{#if demoState.demo}
	<button
		bind:this={btn}
		type="button"
		popovertarget={id}
		aria-expanded={open}
		class="shrink-0 rounded-sm border border-highlight/60 px-1.5 py-0.5 text-xs font-medium text-highlight hover:bg-highlight-soft"
	>
		Demo
	</button>
	<div
		bind:this={pop}
		{id}
		popover="auto"
		ontoggle={(e) => {
			open = (e as ToggleEvent).newState === 'open';
			if (open) {
				place();
				if (!sourceUrl)
					about
						.get()
						.then((a) => (sourceUrl = a.source_url))
						.catch(() => {});
			}
		}}
		class="fixed inset-auto m-0 rounded-md border border-border bg-card p-3 text-left text-xs leading-snug text-foreground shadow-lg"
	>
		<p class="font-medium">A demo of Roastwarden on a simulated roaster</p>
		<p class="mt-1.5 text-muted-foreground">
			{#if demoState.roaster && !demoState.shared}
				The roaster is yours alone: roast, drive a profile, try the alarms. The records, labels and
				reports are shared with everyone trying the demo. It starts over every few hours.
			{:else}
				Everyone here shares the roaster and sees the same roasts live. It starts over every few
				hours.
			{/if}
			Nothing reaches a real roaster.
		</p>
		<div class="mt-2.5 flex flex-col gap-1.5">
			<a
				href={INSTALL_URL}
				rel="external"
				class="font-medium text-action underline-offset-2 hover:underline">Install your own →</a
			>
			<a
				href={feedbackHref(page.url.pathname)}
				rel="external"
				class="font-medium text-action underline-offset-2 hover:underline"
				title="A mail to {FEEDBACK_EMAIL}"
			>
				Send feedback
			</a>
			<span class="text-muted-foreground">
				What would you change? What is missing for your roaster? Mail <span class="select-all"
					>{FEEDBACK_EMAIL}</span
				>, or ask in the
				<a
					href={DISCUSSIONS_URL}
					rel="external"
					class="underline decoration-dotted underline-offset-2 hover:text-foreground"
					>discussions</a
				>.
			</span>
			<span class="flex gap-3 text-muted-foreground">
				<a
					href={SITE_URL}
					rel="external"
					class="underline decoration-dotted underline-offset-2 hover:text-foreground"
					>roastwarden.com</a
				>
				{#if sourceUrl}
					<a
						href={sourceUrl}
						rel="external"
						class="underline decoration-dotted underline-offset-2 hover:text-foreground"
						>Source code</a
					>
				{/if}
			</span>
		</div>
	</div>
{/if}
