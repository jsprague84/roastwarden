<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
</script>

<div class="flex min-h-[60vh] flex-col items-center justify-center text-center">
	<p class="text-6xl font-bold text-muted-foreground">{page.status}</p>

	{#if page.status === 404}
		<h1 class="mt-4 text-2xl font-semibold text-foreground">Page not found</h1>
		<p class="mt-2 text-muted-foreground">
			The page you're looking for doesn't exist or has been moved.
		</p>
	{:else}
		<h1 class="mt-4 text-2xl font-semibold text-foreground">Something went wrong</h1>
		<p class="mt-2 text-muted-foreground">
			{page.error?.message ?? 'An unexpected error occurred.'}
		</p>
	{/if}

	<a
		href={resolve('/')}
		class="mt-6 rounded-lg bg-action px-4 py-2 text-sm font-medium text-action-foreground transition-colors hover:bg-action-hover"
	>
		Back to the roast
	</a>
</div>
