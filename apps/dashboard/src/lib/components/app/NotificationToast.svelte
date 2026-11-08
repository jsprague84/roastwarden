<script lang="ts">
	import { notifications, type Notification } from '#lib/stores/notifications.js';

	const typeStyles: Record<Notification['type'], string> = {
		error: 'border-red-500/30 bg-red-500/10 text-red-400',
		warning: 'border-yellow-500/30 bg-yellow-500/10 text-yellow-400',
		success: 'border-green-500/30 bg-green-500/10 text-green-400'
	};
</script>

{#if $notifications.length > 0}
	<div class="fixed bottom-16 right-4 z-50 flex flex-col gap-2">
		{#each $notifications as notif (notif.id)}
			<div
				class="flex items-start gap-2 rounded-lg border px-4 py-3 shadow-lg {typeStyles[
					notif.type
				]}"
				role="alert"
			>
				<span class="flex-1 text-sm">{notif.message}</span>
				{#if notif.action}
					<button
						onclick={() => {
							notif.action?.callback();
							notifications.dismiss(notif.id);
						}}
						class="shrink-0 rounded bg-action px-2 py-0.5 text-xs font-medium text-action-foreground hover:bg-action-hover"
					>
						{notif.action.label}
					</button>
				{/if}
				<button
					onclick={() => notifications.dismiss(notif.id)}
					class="ml-1 shrink-0 text-current opacity-60 hover:opacity-100"
					aria-label="Dismiss notification"
				>
					&times;
				</button>
			</div>
		{/each}
	</div>
{/if}
