/**
 * Roast profiles (docs/profiles.md): points, events, background curves, roast guidance, auto DROP,
 * and a profile saved from a roast.
 */

import type { HistoryRoast } from '#lib/guidance/expect.js';
import { request } from '#lib/api/http.js';
import type { RoasterKind } from '#lib/api/roasters.js';

export interface RoastProfile {
	id: string;
	name: string;
	description: string | null;
	target_total_time: number | null;
	/** Seconds from CHARGE to first crack. */
	target_first_crack?: number | null;
	target_end_temp: number | null;
	preheat_temp?: number | null;
	charge_temp: number | null;
	/** What it was made for (Artisan's beans, weight in, roastertype). */
	beans?: string | null;
	/** Green weight, g. */
	batch_size_g?: number | null;
	roaster?: string | null;
	/** Roast guidance (docs/roast-guidance.md): SCA tile key, `light` etc. */
	roast_level?: string | null;
	/** espresso | filter | omni | other */
	roast_purpose?: string | null;
	/** Overrides of the level's expected ranges. */
	expectations?: ProfileExpectations | null;
	/** Auto DROP (docs/replay.md): Profile drive marks DROP by itself at the profile's drop; null is off. */
	auto_drop?: AutoDropBy | null;
	/** The roaster kind it was written for (docs/profiles.md); null fits any. */
	roaster_kind?: RoasterKind | null;
	created_at: string;
	updated_at?: string;
}

/** What a profile's auto DROP goes by: its drop time, its drop BT, or its development time after first crack. */
export type AutoDropBy = 'time' | 'bt' | 'development';

/** `{ weight_loss_pct: [14, 16], development_s: [60, 120], dtr_pct, color_ground }`. */
export type ProfileExpectations = Partial<
	Record<'weight_loss_pct' | 'development_s' | 'dtr_pct' | 'color_ground', [number, number]>
>;

export interface ProfileWithPoints extends RoastProfile {
	points: ProfilePoint[];
	/** Replayable events; absent on profiles from older servers. */
	events?: ProfileEvent[];
	/** Background curves recorded from a roast (1 value per interval, from CHARGE). */
	curves?: ProfileCurves | null;
}

export interface ProfilePoint {
	id: string;
	time_seconds: number;
	target_temp: number;
	fan_speed: number | null;
	notes: string | null;
	target_env_temp: number | null;
	/** Heater feedforward, 0-100 %. */
	heater_pct?: number | null;
}

type TriggerKind = 'time' | 'bt';

export type ProfileAction = 'setpoint' | 'fan' | 'heater' | 'mode' | 'landmark' | 'note';

/** Fires at `at_seconds` from CHARGE or when BT rises through `at_bt` (°C). */
export interface ProfileEventInput {
	trigger_kind: TriggerKind;
	at_seconds: number | null;
	at_bt: number | null;
	action: ProfileAction;
	/** setpoint °C, fan %, heater %. */
	value: number | null;
	/** mode auto/manual, landmark event type, note text. */
	label: string | null;
}

interface ProfileEvent extends ProfileEventInput {
	id: string;
	profile_id: string;
	created_at: string;
}

export interface ProfileCurves {
	source_session_id: string | null;
	interval_seconds: number;
	bt: (number | null)[];
	et: (number | null)[];
	ror: (number | null)[];
	heater: (number | null)[];
	fan: (number | null)[];
}

export interface CreateProfileRequest {
	name: string;
	description?: string;
	target_total_time?: number;
	target_first_crack?: number;
	target_end_temp?: number;
	preheat_temp?: number;
	charge_temp?: number;
	beans?: string;
	batch_size_g?: number;
	roaster?: string;
	/** Omit to keep on update; null clears. */
	roast_level?: string | null;
	roast_purpose?: string | null;
	expectations?: ProfileExpectations | null;
	/** Omit to keep on update; null switches it off. */
	auto_drop?: AutoDropBy | null;
	/** The roaster kind it is for; null for any. On update, absent keeps it. */
	roaster_kind?: RoasterKind | null;
	points: {
		time_seconds: number;
		target_temp: number;
		fan_speed?: number;
		notes?: string;
		target_env_temp?: number;
		heater_pct?: number;
	}[];
	/** Omit to keep the stored events on update. */
	events?: ProfileEventInput[];
	/** Omit to keep stored curves on update; null removes them. */
	curves?: ProfileCurves | null;
}

export const profiles = {
	list: () => request<RoastProfile[]>('/api/profiles?include_private=true'),

	get: (id: string) => request<ProfileWithPoints>(`/api/profiles/${id}`),

	create: (req: CreateProfileRequest) =>
		request<ProfileWithPoints>('/api/profiles', {
			method: 'POST',
			body: JSON.stringify(req)
		}),

	update: (id: string, req: CreateProfileRequest) =>
		request<ProfileWithPoints>(`/api/profiles/${id}`, {
			method: 'PUT',
			body: JSON.stringify(req)
		}),

	delete: (id: string) => request<void>(`/api/profiles/${id}`, { method: 'DELETE' }),

	/** The profile's completed roasts on a roaster, newest first (roast guidance). */
	history: (id: string, deviceId?: string) =>
		request<HistoryRoast[]>(
			`/api/profiles/${id}/history${deviceId ? `?device_id=${encodeURIComponent(deviceId)}` : ''}`
		),

	importArtisan: (alogContent: string, name?: string) =>
		request<RoastProfile>('/api/profiles/import/artisan', {
			method: 'POST',
			body: JSON.stringify({ alog_content: alogContent, name })
		}),

	/** Save a recorded session as a profile (points, events and curves, from CHARGE to DROP). */
	fromSession: (sessionId: string, opts: { name?: string; description?: string } = {}) =>
		request<ProfileWithPoints>(`/api/sessions/${sessionId}/profile`, {
			method: 'POST',
			body: JSON.stringify(opts)
		})
};
