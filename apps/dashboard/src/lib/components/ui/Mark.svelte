<script lang="ts">
	// Roastwarden's mark, "Warden" (docs/ui-structure.md, "Visual direction"): the shield in the
	// text colour keeping the amber flame. The same drawing as crates/server/assets/brand/mark.svg
	// and static/favicon.svg; Lucide's 24 px grid, so it sits beside the app's icons.
	let { class: cls = 'h-5 w-5' }: { class?: string } = $props();
</script>

<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" class={cls} aria-hidden="true">
	<path
		d="M12 2.6 4.8 5.4v5.4c0 4.7 3 8.5 7.2 10.6 4.2-2.1 7.2-5.9 7.2-10.6V5.4Z"
		fill="none"
		stroke="currentColor"
		stroke-width="2"
		stroke-linejoin="round"
	/>
	<path
		d="M12 6.6q.6 2.6 2.4 4.2t1.8 3.6a4.2 4.2 0 0 1-8.4 0 3 3 0 0 1 .6-1.8.6.6 0 0 0 3 0c0-1.2-.9-1.8-.9-3q0-1.2 1.5-3"
		fill="#f59e0b"
	/>
</svg>
