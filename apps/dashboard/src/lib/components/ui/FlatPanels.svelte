<script lang="ts">
	/** Renders its Panels flat: for tab bodies, where the tab is the header. */
	import { setContext, type Snippet } from 'svelte';
	import { FLAT_PANELS } from './flat.js';

	let { children }: { children: Snippet } = $props();
	setContext(FLAT_PANELS, true);
</script>

{@render children()}
