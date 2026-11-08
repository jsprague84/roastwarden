<script lang="ts">
	/**
	 * The SCA colour tiles, lightest to darkest, with the profile's level outlined: where a
	 * roast level sits among the others (docs/roast-guidance.md). Fixed coffee tones, the same
	 * in both themes, like the scope's series colours.
	 */
	import { LEVELS, bandText } from '#lib/guidance/references.js';

	let { level = null, class: cls = '' }: { level?: string | null; class?: string } = $props();
	const TONE = ['#d9b98c', '#c79a64', '#ad7b45', '#8f5f32', '#734725', '#57331b', '#3b2213'];
</script>

<div class="flex items-center gap-2 {cls}">
	<div
		class="flex h-2.5 w-36 shrink-0 overflow-hidden rounded-sm"
		role="img"
		aria-label="Roast level: {LEVELS.find((l) => l.key === level)?.label ?? 'not set'}"
	>
		{#each LEVELS as l, i (l.key)}
			<span
				class="h-full flex-1 {l.key === level
					? 'outline outline-2 -outline-offset-2 outline-foreground'
					: level
						? 'opacity-45'
						: ''}"
				style="background: {TONE[i]}"
				title="{l.label}: ground Agtron {bandText(l.agtron)}"
			></span>
		{/each}
	</div>
</div>
