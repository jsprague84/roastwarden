<script lang="ts">
	/**
	 * PID autotune: the server's test (Autotune 2, docs/autotune-2.md). The firmware's own
	 * relay test went in firmware 3.5.0; its stored runs stay in the run history. Run
	 * history, the gain schedule and stored-gain undo live in their own panels next to this one.
	 */
	import { Zap } from '@lucide/svelte';
	import { deviceId, telemetry } from '#lib/stores/telemetry.js';
	import { sessions, type RoastSession } from '#lib/api/sessions.js';
	import type { Gains } from '#lib/api/tuning.js';
	import ServerAutotune from '#lib/components/roasters/ServerAutotune.svelte';

	let hasActiveSession = $state(false);
	const current = $derived<Gains | null>(
		$telemetry?.Kp != null && $telemetry?.Ki != null && $telemetry?.Kd != null
			? { kp: $telemetry.Kp, ki: $telemetry.Ki, kd: $telemetry.Kd }
			: null
	);

	$effect(() => {
		checkActiveSessions();
		const interval = setInterval(checkActiveSessions, 5000);
		return () => clearInterval(interval);
	});

	async function checkActiveSessions() {
		try {
			const all = await sessions.list(undefined, 10);
			hasActiveSession = all.some(
				(s: RoastSession) => s.status === 'active' || s.status === 'paused'
			);
		} catch {
			hasActiveSession = false;
		}
	}
</script>

<div class="rounded-lg border border-border bg-card p-4">
	<div class="mb-3 flex items-center gap-2">
		<Zap class="h-5 w-5 text-amber-500" />
		<h2 class="text-lg font-semibold text-foreground">PID Autotune</h2>
	</div>

	{#if !$deviceId}
		<p class="text-sm text-muted-foreground">
			No device connected. Connect a roaster to use autotune.
		</p>
	{:else}
		<ServerAutotune {hasActiveSession} {current} />
	{/if}
</div>
