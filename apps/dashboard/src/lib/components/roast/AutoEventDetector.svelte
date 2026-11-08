<script lang="ts">
	/**
	 * The first-crack prompt (DRY end, the turning point and DROP are marked on the server,
	 * alarms.rs). Once dry end is marked and BT climbs to within 2 °C of the profile's
	 * first-crack BT (else 196 °C), a toast says first crack is due, with a button to mark it.
	 * It goes by temperature, not by a dip in RoR: under closed-loop control the PID hides
	 * the dip, and on the real roasts the RoR rule fired 45 s to 2½ minutes late.
	 */
	import { telemetry, rateOfRise } from '#lib/stores/telemetry.js';
	import { events as eventsApi } from '#lib/api/events.js';
	import type { RoastSession } from '#lib/api/sessions.js';
	import { settings } from '#lib/api/settings.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { profileState } from '#lib/stores/profile.svelte.js';
	import { fcDue } from '#lib/roast/live.js';
	import { fmtTemp, tempUnit } from '#lib/stores/units.svelte.js';

	let {
		activeSession,
		events = [],
		onmarked
	}: {
		activeSession: RoastSession | null;
		events?: Array<{ event_type: string }>;
		onmarked?: () => void;
	} = $props();

	// Landmarks already marked (by hand, by the server, or in an earlier page
	// load) must not be suggested again.
	const dryMarked = $derived(events.some((e) => e.event_type === 'drying_end'));
	const fcMarked = $derived(events.some((e) => e.event_type === 'first_crack_start'));

	let enabled = $state(true);
	let promptedFor = $state<string | null>(null);

	$effect(() => {
		settings.get().then((s) => {
			enabled = s['auto_event_detection'] !== 'false';
		});
	});

	// The profile's first-crack BT, when it has that landmark.
	const fcTarget = $derived(
		profileState.activeProfile?.events?.find(
			(e) => e.action === 'landmark' && e.label === 'first_crack_start'
		)?.at_bt ?? null
	);

	$effect(() => {
		const s = activeSession;
		if (!enabled || !s || s.status !== 'active' || promptedFor === s.id) return;
		if (fcMarked || !dryMarked) return;
		const bt = $telemetry?.beanTemp;
		if (!fcDue(bt, $rateOfRise, fcTarget)) return;
		promptedFor = s.id;
		const target = fcTarget ?? 196;
		notifications.add(
			`First crack due: BT ${fmtTemp(bt ?? null, 0)}${tempUnit()}, the profile cracks near ${fmtTemp(target, 0)}${tempUnit()}. Listen, and mark it at steady cracking.`,
			'warning',
			30000,
			{
				label: 'Mark first crack',
				callback: () => {
					const start = s.start_time ? new Date(s.start_time).getTime() : Date.now();
					eventsApi
						.create(s.id, {
							event_type: 'first_crack_start',
							elapsed_seconds: Math.round(Math.max(0, (Date.now() - start) / 1000)),
							temperature: $telemetry?.beanTemp ?? undefined
						})
						.then(() => onmarked?.())
						.catch(() => notifications.add('Failed to mark first crack event', 'error'));
				}
			}
		);
	});
</script>
