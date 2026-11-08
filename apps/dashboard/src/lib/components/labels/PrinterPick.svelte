<script lang="ts">
	/** Which printer a print goes to (docs/labels.md, "Printers"), with a tray and colour for
	 *  this print: shown when there is more than one printer, or when asked to. */
	import { labelStudio, type LabelPrinter } from '#lib/api/labelStudio.js';
	import { onMount } from 'svelte';

	let {
		printer = $bindable(''),
		tray = $bindable(''),
		color = $bindable(''),
		printers = $bindable<LabelPrinter[]>([])
	}: { printer?: string; tray?: string; color?: string; printers?: LabelPrinter[] } = $props();

	onMount(async () => {
		try {
			printers = await labelStudio.printers();
			if (!printer) printer = printers.find((p) => p.is_default)?.id ?? printers[0]?.id ?? '';
		} catch {
			printers = [];
		}
	});
	const chosen = $derived(printers.find((p) => p.id === printer));
	const field =
		'mt-1 w-full rounded border border-border bg-input px-2 py-1.5 text-sm text-foreground';
	const lbl = 'block text-xs font-medium text-muted-foreground';
</script>

{#if printers.length > 1}
	<label class={lbl}
		>Printer
		<select class={field} bind:value={printer}>
			{#each printers as p (p.id)}<option value={p.id}
					>{p.name}{p.is_default ? ' (default)' : ''}</option
				>{/each}
		</select>
	</label>
{/if}
{#if chosen}
	<details class="text-sm">
		<summary class="cursor-pointer text-xs text-muted-foreground"
			>Tray and colour for this print</summary
		>
		<div class="mt-2 grid grid-cols-2 gap-3">
			<label class={lbl}
				>Tray
				<select class={field} bind:value={tray}>
					<option value=""
						>{chosen.tray ? `${chosen.tray} (its own)` : 'The one holding the sheet'}</option
					>
					{#each chosen.info?.media_sources ?? [] as t (t)}<option value={t}>{t}</option>{/each}
				</select>
			</label>
			<label class={lbl}
				>Colour
				<select class={field} bind:value={color}>
					<option value=""
						>{chosen.color ? `${chosen.color} (its own)` : 'Black unless coloured'}</option
					>
					<option value="monochrome">Black</option>
					<option value="color">Colour</option>
				</select>
			</label>
		</div>
	</details>
{/if}
