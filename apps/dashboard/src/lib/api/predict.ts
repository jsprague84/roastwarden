/**
 * The roaster's own model (docs/prediction.md): the live prediction, the RoR warnings, the model
 * and its scores.
 */

import { request } from '#lib/api/http.js';

interface HorizonError {
	horizon_s: number;
	rmse: number;
	/** 90 % of the predictions were within this, °C */
	p90: number;
	bias: number;
	n: number;
}

type PredictMode = 'recorded' | 'plan' | 'held' | 'projection';

interface ThermalModel {
	tau_s: number;
	gain: number;
	knot_from_c: number;
	knot_step_c: number;
	drift: number[];
}

interface StoredModel {
	id: string;
	device_id: string;
	fitted_at: string;
	model: ThermalModel;
	quality: {
		roasts: number;
		samples: number;
		modes: { mode: PredictMode; horizons: HorizonError[] }[];
	};
	roasts: number;
	samples: number;
	session_ids: string[];
}

interface LandmarkScore {
	actual_t: number;
	predicted_t: number;
	made_at_t: number;
	error_s: number;
	mae_s: number;
	n: number;
}

/** The RoR watch's warnings (docs/prediction.md, "RoR warnings"). */
type RorWarningKind = 'crash' | 'flick' | 'stall';

export interface RorWarning {
	kind: RorWarningKind;
	/** When it is expected to start (or started, when seen), s since CHARGE */
	t: number;
	/** Seen in the RoR now; false: expected by the model */
	seen: boolean;
	/** Since when the predictions have said so, s since CHARGE */
	since: number;
	reason: string;
}

interface RorWatch {
	t: number;
	/** RoR now, °C/min (BT's slope over the last 30 s) */
	now?: number | null;
	/** °C/min per minute */
	trend?: number | null;
	at_60?: number | null;
	at_120?: number | null;
	/** From DRY end (or BT 160 °C) */
	armed: boolean;
	warnings: RorWarning[];
}

interface RorWarningScore {
	kind: RorWarningKind;
	warned_at: number;
	expected_t: number;
	seen: boolean;
	reason: string;
	last_t: number;
	came_t?: number | null;
	lead_s?: number | null;
}

export interface RoastScore {
	session_id: string;
	predictions: number;
	model: HorizonError[];
	projection: HorizonError[];
	beats_projection?: boolean | null;
	first_crack?: LandmarkScore | null;
	drop?: LandmarkScore | null;
	ror_warnings?: RorWarningScore[];
	ror_events?: RorWarning[];
}

export interface ModelView {
	device_id: string;
	source: 'fit' | 'prior';
	model: ThermalModel;
	fit?: StoredModel | null;
	settles: { heater_pct: number; fan_pct: number; bt?: number | null }[];
	recent: (RoastScore & { name: string; batch_number?: number | null })[];
	pooled: {
		roasts: number;
		model_120?: HorizonError | null;
		projection_120?: HorizonError | null;
		beats_projection?: boolean | null;
	};
}

interface PredictedLandmark {
	/** s since CHARGE */
	t: number;
	low: number;
	high: number;
	bt?: number | null;
	reason: string;
}

export interface Prediction {
	/** Now, s since CHARGE */
	t: number;
	bt: number;
	plan: 'held' | 'setpoint' | 'drive';
	mode: PredictMode;
	reason: string;
	points: { t: number; bt: number; low: number; high: number; heater: number }[];
	bt_60: number;
	bt_120: number;
	band_120: number;
	projection_60?: number | null;
	projection_120?: number | null;
	heater_limit?: { t: number; behind_c?: number | null } | null;
	first_crack?: PredictedLandmark | null;
	drop?: PredictedLandmark | null;
	ror?: RorWatch | null;
}

export const prediction = {
	/** The live prediction (404 outside CHARGE + 30 s … DROP). */
	live: (deviceId: string) =>
		request<Prediction>(`/api/roaster/${encodeURIComponent(deviceId)}/prediction`),
	model: (deviceId: string) =>
		request<ModelView>(`/api/roaster/${encodeURIComponent(deviceId)}/model`),
	fit: (deviceId: string) =>
		request<ModelView>(`/api/roaster/${encodeURIComponent(deviceId)}/model/fit`, {
			method: 'POST',
			body: '{}'
		}),
	roast: (sessionId: string) =>
		request<RoastScore>(`/api/sessions/${encodeURIComponent(sessionId)}/prediction`)
};
