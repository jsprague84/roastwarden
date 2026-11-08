<script lang="ts">
	/** What a page of an off module shows instead of itself (docs/lean-home.md). */
	import { resolve } from '$app/paths';
	import { MODULE_INFO, type Module } from '#lib/modules/modules.js';

	let { module }: { module: Module } = $props();
</script>

<p class="text-sm text-muted-foreground">
	{MODULE_INFO[module].off}:
	<a
		href={resolve('/settings/modules')}
		class="font-medium text-foreground underline underline-offset-2">Settings → Modules</a
	>
</p>
