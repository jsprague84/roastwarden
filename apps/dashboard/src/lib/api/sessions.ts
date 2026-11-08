/**
 * Roasts (sessions): the record, its stored telemetry, and a roast made, started, paused,
 * completed, cancelled or imported.
 */

import { request } from '#lib/api/http.js';
import type { ProfileWithPoints } from '#lib/api/profiles.js';
import type { RoasterChannel } from '#lib/api/roasters.js';

export interface RoastSession {
	id: string;
	name: string;
	device_id: string;
	profile_id: string | null;
	status: string;
	start_time: string | null;
	end_time: string | null;
	created_at: string;
	bean_origin: string | null;
	bean_variety: string | null;
	green_weight: number | null;
	roasted_weight: number | null;
	notes: string | null;
	target_roast_level?: string | null;
	ambient_temp?: number | null;
	humidity?: number | null;
	total_time_seconds: number | null;
	max_temp: number | null;
	first_crack_time: number | null;
	development_time_ratio: number | null;
	weight_loss_pct: number | null;
	max_ror: number | null;
	avg_ror_drying: number | null;
	avg_ror_maillard: number | null;
	avg_ror_development: number | null;
	drying_end_time: number | null;
	drying_end_temp: number | null;
	auc_value: number | null;
	// Roast properties (migration 016)
	roast_purpose?: string | null;
	color_whole?: number | null;
	color_ground?: number | null;
	color_scale?: string | null;
	moisture_green?: number | null;
	moisture_roasted?: number | null;
	density_green?: number | null;
	/** Comma list, see lib/roast/properties.ts. */
	roast_flags?: string | null;
	/** 1..5 */
	rating?: number | null;
	/** BT vs the session profile, TP..DROP, °C. */
	profile_rmse?: number | null;
	profile_max_dev?: number | null;
	profile_bias?: number | null;
	// Batches and beans (migration 018, docs/batches.md)
	bean_id?: string | null;
	/** Per roaster, from 1. */
	batch_number?: number | null;
	planned_roast_id?: string | null;
}

export interface CreateSessionRequest {
	name: string;
	device_id: string;
	profile_id?: string;
	/** A bean from stock: the roast draws its green weight from it. */
	bean_id?: string;
	/** The planned roast this starts: fills what the request leaves out. */
	planned_roast_id?: string;
	bean_origin?: string;
	bean_variety?: string;
	green_weight?: number;
	notes?: string;
}

export interface UpdateSessionRequest {
	name?: string;
	bean_origin?: string;
	bean_variety?: string;
	green_weight?: number;
	roasted_weight?: number;
	target_roast_level?: string;
	ambient_temp?: number;
	humidity?: number;
	notes?: string;
	first_crack_time?: number;
	development_time_ratio?: number;
	// Roast properties: null clears.
	roast_purpose?: string | null;
	color_whole?: number | null;
	color_ground?: number | null;
	color_scale?: string | null;
	moisture_green?: number | null;
	moisture_roasted?: number | null;
	density_green?: number | null;
	roast_flags?: string | null;
	rating?: number | null;
	/** null clears (the beans go back to stock). */
	bean_id?: string | null;
	/** null or 0 clears. */
	batch_number?: number | null;
}

/** Telemetry data point stored with a roast session (matches backend SessionTelemetry). */
export interface SessionTelemetryPoint {
	id: string;
	session_id: string;
	timestamp: string;
	elapsed_seconds: number;
	bean_temp: number | null;
	env_temp: number | null;
	rate_of_rise: number | null;
	/** % */
	heater: number | null;
	/** % */
	fan: number | null;
	setpoint: number | null;
	/** The control fields and the roaster's declared channels the sample kept (docs/channels.md). */
	extras?: Record<string, unknown> | null;
}

/** Session detail response from GET /api/sessions/:id (matches backend SessionWithTelemetry). */
export interface SessionWithTelemetry extends RoastSession {
	telemetry: SessionTelemetryPoint[];
	profile: ProfileWithPoints | null;
	/** The roast's declared channels: the roaster's list when it started, or (a roast from
	 * before migration 049) the roaster's list now (docs/channels.md). */
	channels: RoasterChannel[];
}

export const sessions = {
	/** Edit metadata after the fact; weight loss is recomputed from the weights. */
	update: (id: string, req: UpdateSessionRequest) =>
		request<RoastSession>(
			`/api/sessions/${id}`,
			{
				method: 'PUT',
				body: JSON.stringify(req)
			},
			true
		),

	create: (req: CreateSessionRequest) =>
		request<RoastSession>('/api/sessions', {
			method: 'POST',
			body: JSON.stringify(req)
		}),

	list: (deviceId?: string, limit?: number) => {
		const params = new URLSearchParams();
		if (deviceId) params.set('device_id', deviceId);
		if (limit) params.set('limit', String(limit));
		const qs = params.toString();
		return request<RoastSession[]>(`/api/sessions${qs ? `?${qs}` : ''}`);
	},

	get: (id: string) => request<SessionWithTelemetry>(`/api/sessions/${id}`),

	start: (id: string) => request<RoastSession>(`/api/sessions/${id}/start`, { method: 'POST' }),

	pause: (id: string) => request<RoastSession>(`/api/sessions/${id}/pause`, { method: 'POST' }),

	resume: (id: string) => request<RoastSession>(`/api/sessions/${id}/resume`, { method: 'POST' }),

	complete: (id: string) =>
		request<RoastSession>(`/api/sessions/${id}/complete`, { method: 'POST' }),

	delete: (id: string) => request<void>(`/api/sessions/${id}`, { method: 'DELETE' }),

	/** Stop the drive, heater power off, cool if hot, delete the roast (docs/roast-sequence.md). */
	cancel: (id: string) => request<void>(`/api/sessions/${id}/cancel`, { method: 'POST' }),

	/** Import a recorded Artisan roast (.alog) as a completed session. */
	importArtisan: (alogContent: string, deviceId?: string, name?: string) =>
		request<RoastSession>(
			'/api/sessions/import/artisan',
			{
				method: 'POST',
				body: JSON.stringify({ alog_content: alogContent, device_id: deviceId, name })
			},
			true
		)
};
