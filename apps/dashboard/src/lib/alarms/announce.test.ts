import { describe, it, expect } from 'vitest';
import { alarmKey, shouldAnnounce, ANNOUNCE_WITHIN_MS } from './announce.js';
import type { AlarmEvent } from '#lib/stores/telemetry.js';

const alarm = (over: Partial<AlarmEvent> = {}): AlarmEvent => ({
	device_id: 'esp32_roaster_01',
	session_id: 's15',
	session_name: 'Rwanda B',
	name: 'RoR flick',
	condition_type: 'ror_flick',
	threshold: 0,
	message: 'RoR rising after first crack',
	value: 7.2,
	elapsed_seconds: 535,
	fired_at: '2026-11-06T03:10:00Z',
	received_at: 1_000_000,
	...over
});

describe('shouldAnnounce', () => {
	it('announces a fresh alarm for the roast on screen', () => {
		expect(shouldAnnounce(alarm(), null, 's15', 1_000_500)).toBe(true);
	});

	it('does not announce the same alarm again when the Roast page opens again', () => {
		const a = alarm();
		expect(shouldAnnounce(a, alarmKey(a), 's15', 1_000_500)).toBe(false);
		expect(shouldAnnounce(a, alarmKey(a), null, 1_000_500)).toBe(false);
	});

	it('does not announce an alarm heard while the page was closed', () => {
		expect(shouldAnnounce(alarm(), null, null, 1_000_000 + ANNOUNCE_WITHIN_MS + 1)).toBe(false);
	});

	it("leaves another roast's alarm to the notification channels", () => {
		expect(shouldAnnounce(alarm({ session_id: 'other' }), null, 's15', 1_000_500)).toBe(false);
	});

	it('announces a new firing of the same alarm', () => {
		const first = alarm();
		const again = alarm({ fired_at: '2026-11-06T03:11:00Z', received_at: 1_060_000 });
		expect(shouldAnnounce(again, alarmKey(first), 's15', 1_060_100)).toBe(true);
	});
});
