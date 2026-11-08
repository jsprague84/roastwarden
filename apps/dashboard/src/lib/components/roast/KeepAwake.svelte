<script lang="ts">
	/**
	 * Keeps the screen on while a roast runs (Screen Wake Lock), so a phone on the bench does
	 * not sleep mid-roast. The browser drops the lock when the page is hidden; it is taken
	 * again when the page shows. Nothing happens where the API is missing or refused.
	 */
	let { on }: { on: boolean } = $props();

	$effect(() => {
		if (!on || typeof navigator === 'undefined' || !('wakeLock' in navigator)) return;
		let lock: WakeLockSentinel | null = null;
		let stopped = false;
		const take = async () => {
			if (stopped || document.visibilityState !== 'visible' || (lock && !lock.released)) return;
			try {
				lock = await navigator.wakeLock.request('screen');
			} catch {
				// Refused (battery saver, no user gesture yet): the screen may sleep.
			}
		};
		const onVisible = () => void take();
		document.addEventListener('visibilitychange', onVisible);
		void take();
		return () => {
			stopped = true;
			document.removeEventListener('visibilitychange', onVisible);
			void lock?.release().catch(() => {});
		};
	});
</script>
