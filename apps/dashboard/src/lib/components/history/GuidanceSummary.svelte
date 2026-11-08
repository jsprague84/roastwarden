<script lang="ts">
	/** The roast summary (docs/roast-guidance.md): at most three lines naming what is off.
	 * Nothing off, nothing shown. */
	let {
		lines,
		note = null,
		compact = false,
		class: cls = ''
	}: {
		lines: string[];
		note?: string | null;
		/** One line each, the rest on hover (the Roast page). */ compact?: boolean;
		class?: string;
	} = $props();
</script>

{#if lines.length || note}
	<div class="space-y-0.5 text-sm {cls}" aria-label="Roast summary">
		{#each lines as line (line)}
			<p class="flex gap-2 text-foreground" title={compact ? line : undefined}>
				<span
					class="{compact ? 'mt-[5px]' : 'mt-[7px]'} h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500"
					aria-hidden="true"
				></span><span class={compact ? 'min-w-0 truncate' : ''}>{line}</span>
			</p>
		{/each}
		{#if note}<p class="text-xs text-muted-foreground">{note}</p>{/if}
	</div>
{/if}
