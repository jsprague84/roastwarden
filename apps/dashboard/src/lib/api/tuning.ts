/**
 * Tuning: autotune runs, the server's own test with its analysis and designs (docs/autotune-2.md),
 * the PID history, the gain schedule.
 */

import { request, roasterPath } from '#lib/api/http.js';

/** Device PID units: Kp per °C on the 0..255 output, Ki per °C·s, Kd per °C/s. */
export interface Gains {
	kp: number;
	ki: number;
	kd: number;
}

interface RuleGains extends Gains {
	rule: string;
	label: string;
	/** No derivative term. */
	pi: boolean;
	/** A gain hit the firmware's limit. */
	clamped: boolean;
}

export interface AutotuneRun {
	id: string;
	device_id: string;
	created_at: string;
	state: 'complete' | 'failed' | string;
	quality: 'good' | 'acceptable' | 'poor' | null;
	message: string | null;
	fan_pct: number | null;
	target_temperature: number | null;
	test_temperature: number | null;
	ku: number | null;
	tu: number | null;
	/** Holding power at the test temperature, %. */
	bias: number | null;
	tuning_method: string | null;
	warnings: string[];
	usable: boolean;
	unusable_reason?: string;
	rules: RuleGains[];
	recommended: Gains | null;
	original: Gains | null;
	payload: Record<string, unknown>;
	/** `firmware` (the roaster's own relay test) or `server` (Autotune 2). */
	source?: 'firmware' | 'server' | string;
	/** A server run's analysis (docs/autotune-2.md). */
	analysis?: TuneAnalysis;
}

/** `k·e^(−l·s)/((1+t1·s)(1+t2·s))`, or `k·e^(−l·s)/s` when integrating (k is then the rate kv). */
interface ProcessModel {
	k: number;
	t1: number;
	t2: number;
	l: number;
	integrating?: boolean;
}

interface Robustness {
	ms: number;
	mt: number;
	/** null: infinite. */
	gain_margin: number | null;
	phase_margin_deg: number | null;
	delay_margin_s: number | null;
	crossover_rad_s: number | null;
	noise_gain: number;
	load_ie: number;
}

export interface TuneDesign {
	rule: 'simc' | 'amigo' | 'zn_pi' | 'tyreus_luyben_pi' | string;
	label: string;
	tau_c?: number;
	/** Process units: % per °C, ki per second. */
	pid: { kp: number; ki: number; kd: number };
	ti: number;
	robustness: Robustness;
	robust_box: { gain_x2: number; gain_half: number; delay_x2: number } | null;
	/** Heater standard deviation from the sensor's noise, %. */
	jitter: number | null;
	step_kick: number | null;
	/** °C behind a 10 °C/min ramp with no lead. */
	ramp_lag: number | null;
	verdict: 'ok' | 'warn' | 'refuse';
	notes: string[];
}

interface TuneFit {
	model: ProcessModel;
	order: 'integrating' | 'first' | 'second';
	offset: number;
	drift: number;
	rmse: number;
	fit_pct: number;
	samples: number;
	aic: number;
	kv: number;
	t1_low: number;
	t1_high: number | null;
	lag_identified: boolean;
	residual_ratio: number | null;
	holdout_rmse: number | null;
}

interface TuneAnalysis {
	version: number;
	quality: 'good' | 'acceptable' | 'poor';
	warnings: { code: string; text: string }[];
	setpoint: number;
	holding_power: number | null;
	fitted_holding_power: number | null;
	bias_drift_per_min: number | null;
	noise: number | null;
	quantum: number | null;
	hysteresis: number;
	berner_tau: number | null;
	gamma: number | null;
	fits: TuneFit[];
	chosen: TuneFit['order'] | null;
	design_model: ProcessModel | null;
	design_basis: string;
	tau: number | null;
	agreement: { gain_ratio: number; phase_diff_deg: number } | null;
	ku: number | null;
	tu: number | null;
	designs: TuneDesign[];
	recommended: number | null;
}

export interface TuneState {
	device_id: string;
	run_id: string;
	active: boolean;
	phase:
		'approach' | 'symmetric' | 'asymmetric' | 'analysing' | 'done' | 'failed' | 'stopped' | string;
	started_at: string;
	elapsed_s: number;
	target: number;
	setpoint: number;
	fan_pct: number | null;
	input: number;
	bias: number;
	hysteresis: number;
	noise: number | null;
	cycles: number;
	samples: number;
	bt: number | null;
	message: string | null;
	result_run_id: string | null;
	quality: string | null;
}

export interface StartTune {
	setpoint: number;
	fan_pct?: number;
	approach_power?: number;
	max_swing?: number;
	asymmetry?: number;
	hysteresis?: number;
}

export interface TuneSamples {
	t: number[];
	u: number[];
	y: number[];
	fan: (number | null)[];
	model?: { t: number[]; y: number[] };
}

export interface PidChange {
	id: number;
	device_id: string;
	created_at: string;
	gains: Gains;
	previous: Gains | null;
	source: 'manual' | 'autotune' | 'autotune_apply' | 'undo' | string;
	run_id: string | null;
	rule: string | null;
	undone: boolean;
}

export interface ScheduleRow extends Gains {
	fan_pct: number;
	/** Holding power at this fan (autotune bias), %. */
	ff?: number;
	run_id?: string;
	rule?: string;
	note?: string;
	/** A server run's model the row's gains were designed on. */
	model?: ProcessModel;
	/** SIMC's knob the row was designed with. */
	tau_c?: number;
}

export interface GainSchedule {
	enabled: boolean;
	rows: ScheduleRow[];
}

export interface GainScheduleView {
	device_id: string;
	schedule: GainSchedule;
	/** Fan % the device reports now. */
	fan_pct: number | null;
	gains_now: Gains | null;
	/** Gains this request sent to the device. */
	applied: Gains | null;
	/** What to look at in the saved schedule, in words. */
	advice?: string[];
}

export type ApplyTarget = 'device' | 'schedule';

export const tuning = {
	runs: (deviceId: string, limit = 50) =>
		request<{ device_id: string; runs: AutotuneRun[] }>(
			`${roasterPath(deviceId)}/autotune/runs?limit=${limit}`
		),

	/** 204 (device) or the updated schedule view (schedule). `tauC`: a server run's SIMC knob. */
	applyRun: (deviceId: string, runId: string, rule: string, to: ApplyTarget, tauC?: number) =>
		request<GainScheduleView | void>(
			`${roasterPath(deviceId)}/autotune/runs/${encodeURIComponent(runId)}/apply`,
			{
				method: 'POST',
				body: JSON.stringify(tauC != null ? { rule, to, tau_c: tauC } : { rule, to })
			},
			true
		),

	run: (deviceId: string, runId: string) =>
		request<AutotuneRun>(`${roasterPath(deviceId)}/autotune/runs/${encodeURIComponent(runId)}`),

	samples: (deviceId: string, runId: string) =>
		request<TuneSamples>(
			`${roasterPath(deviceId)}/autotune/runs/${encodeURIComponent(runId)}/samples`
		),

	/** Preview a design on a server run's model (read-only). */
	design: (deviceId: string, runId: string, rule: string, tauC: number) =>
		request<{ design: TuneDesign; gains: Gains }>(
			`${roasterPath(deviceId)}/autotune/runs/${encodeURIComponent(runId)}/design?rule=${encodeURIComponent(rule)}&tau_c=${tauC}`
		),

	/** The server's test (404 when none since the server started). */
	tuneState: (deviceId: string) => request<TuneState>(`${roasterPath(deviceId)}/tune`),

	tuneStart: (deviceId: string, body: StartTune) =>
		request<TuneState>(
			`${roasterPath(deviceId)}/tune/start`,
			{ method: 'POST', body: JSON.stringify(body) },
			true
		),

	tuneStop: (deviceId: string) =>
		request<TuneState>(`${roasterPath(deviceId)}/tune/stop`, { method: 'POST' }, true),

	pidHistory: (deviceId: string, limit = 20) =>
		request<{ device_id: string; current: Gains | null; changes: PidChange[] }>(
			`${roasterPath(deviceId)}/control/pid/history?limit=${limit}`
		),

	undo: (deviceId: string) =>
		request<{ restored: Gains; undid: number }>(
			`${roasterPath(deviceId)}/control/pid/undo`,
			{ method: 'POST' },
			true
		),

	getSchedule: (deviceId: string) =>
		request<GainScheduleView>(`${roasterPath(deviceId)}/gain_schedule`),

	putSchedule: (deviceId: string, schedule: GainSchedule) =>
		request<GainScheduleView>(
			`${roasterPath(deviceId)}/gain_schedule`,
			{ method: 'PUT', body: JSON.stringify(schedule) },
			true
		)
};
