<script lang="ts" module>
	// The last alarm announced, kept across Roast page visits (the store keeps the newest alarm).
	let seen: string | null = null;
</script>

<script lang="ts">
	import { alarmEvent } from '#lib/stores/telemetry.js';
	import { alarmKey, shouldAnnounce } from '#lib/alarms/announce.js';
	import type { RoastEvent } from '#lib/api/events.js';
	import type { RoastSession } from '#lib/api/sessions.js';
	import { settings } from '#lib/api/settings.js';
	import { notifications } from '#lib/stores/notifications.js';

	// Alarms are evaluated on the server for every active session, so they fire
	// even when no dashboard is open. This component only announces them: a
	// toast plus an optional beep.
	let {
		activeSession = null
	}: { activeSession?: RoastSession | null; sessionEvents?: RoastEvent[] } = $props();

	let soundEnabled = $state(true);
	let audioCtx: AudioContext | null = null;

	function loadAlarmConfig() {
		settings
			.get()
			.then((s) => {
				soundEnabled = s['alarm_sound_enabled'] !== 'false';
			})
			.catch(() => {});
	}

	$effect(() => {
		loadAlarmConfig();
		const handler = () => loadAlarmConfig();
		window.addEventListener('roastwarden:alarms-updated', handler);
		return () => window.removeEventListener('roastwarden:alarms-updated', handler);
	});

	function playBeep() {
		if (!soundEnabled) return;
		try {
			if (!audioCtx) audioCtx = new AudioContext();
			const osc = audioCtx.createOscillator();
			const gain = audioCtx.createGain();
			osc.connect(gain);
			gain.connect(audioCtx.destination);
			osc.frequency.value = 440;
			gain.gain.value = 0.3;
			osc.start();
			osc.stop(audioCtx.currentTime + 0.2);
		} catch {
			// Audio not available
		}
	}

	$effect(() => {
		const alarm = $alarmEvent;
		if (!alarm) return;
		const announce = shouldAnnounce(alarm, seen, activeSession?.id ?? null, Date.now());
		seen = alarmKey(alarm);
		if (!announce) return;
		const did = alarm.actions?.length ? ` → ${alarm.actions.join(', ')}` : '';
		notifications.add(`${alarm.name}: ${alarm.message}${did}`, 'warning', 10000);
		playBeep();
	});
</script>
