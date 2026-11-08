/** Batch #1 "commissioning 2" (production session 8c1bf385…), the acceptance fixture of
 * docs/roast-guidance.md, "Worked example". */
import type { RoastEvent } from '#lib/api/events.js';
import type { RoastSession } from '#lib/api/sessions.js';
import type { HistoryRoast } from './expect.js';

export const batch1: RoastSession = {
	id: '8c1bf385-4566-4bcc-af99-837c11c71fce',
	name: 'commissioning 2',
	device_id: 'esp32_roaster_01',
	profile_id: 'fluidbed-espresso',
	status: 'completed',
	start_time: '2026-10-24T06:18:12Z',
	end_time: '2026-10-24T06:32:13Z',
	created_at: '2026-10-24T06:18:12Z',
	bean_origin: 'Brazil',
	bean_variety: 'dry',
	green_weight: 100,
	roasted_weight: 78.3,
	notes: '',
	target_roast_level: null,
	total_time_seconds: 714,
	max_temp: 212.8,
	first_crack_time: 473,
	development_time_ratio: 0.34477445,
	weight_loss_pct: 21.699997,
	max_ror: 61,
	avg_ror_drying: 29.6,
	avg_ror_maillard: 12.1,
	avg_ror_development: 4.9,
	drying_end_time: 243,
	drying_end_temp: 150,
	auc_value: 2109.2,
	roast_purpose: null,
	color_ground: null,
	color_scale: null,
	moisture_green: null,
	batch_number: 1
};

const ev = (event_type: string, elapsed_seconds: number, temperature: number): RoastEvent => ({
	id: event_type,
	session_id: batch1.id,
	event_type,
	elapsed_seconds,
	temperature,
	notes: null,
	created_at: ''
});

export const batch1Events: RoastEvent[] = [
	ev('charge', 5.4, 25),
	ev('drying_end', 243.322, 150),
	ev('first_crack_start', 473.1, 194.5),
	ev('first_crack_end', 653.7, 210),
	ev('drop', 719.2, 212.3),
	ev('cool_end', 835.32, 44.8)
];

/** The profile as the worked example sets it: Light espresso, level light. */
export const lightEspresso = {
	roast_level: 'light',
	roast_purpose: 'espresso',
	expectations: null,
	target_first_crack: 540
};

/** A history row like the server's, for a roast of this profile. */
export function historyRow(id: string, o: Partial<HistoryRoast> = {}): HistoryRoast {
	return {
		session_id: id,
		name: id,
		device_id: 'esp32_roaster_01',
		batch_number: null,
		roasted_at: '2026-10-24T06:18:12Z',
		fc_s: 540,
		fc_bt: 200,
		drop_s: 660,
		drop_bt: 210,
		dry_s: 240,
		dry_bt: 150,
		development_s: 120,
		delta_t_fc_drop: 10,
		total_s: 660,
		weight_loss_pct: 15,
		color_ground: 75,
		color_scale: 'agtron',
		rmse: 1,
		...o
	};
}
