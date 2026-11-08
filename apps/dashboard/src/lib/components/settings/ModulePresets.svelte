<script lang="ts">
	/** The three module presets, each with its line (Settings → Modules and /welcome). */
	import { PRESETS, PRESET_INFO, type Preset } from '#lib/modules/modules.js';

	let {
		value,
		onpick,
		disabled = false
	}: { value: Preset | 'custom'; onpick: (p: Preset) => void; disabled?: boolean } = $props();
	const presets = Object.keys(PRESETS) as Preset[];
</script>

<div class="grid gap-2 sm:grid-cols-3" role="radiogroup" aria-label="What kind of roasting">
	{#each presets as p (p)}
		<button
			type="button"
			role="radio"
			aria-checked={value === p}
			{disabled}
			onclick={() => onpick(p)}
			class="rounded-md border px-3 py-2 text-left text-sm disabled:opacity-50 {value === p
				? 'border-selected ring-1 ring-selected'
				: 'border-border hover:bg-accent'}"
		>
			<span class="font-medium text-foreground">{PRESET_INFO[p].name}</span>
			<span class="mt-0.5 block text-xs text-muted-foreground">{PRESET_INFO[p].line}</span>
		</button>
	{/each}
</div>
