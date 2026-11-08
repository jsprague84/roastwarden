/**
 * Profile drive (docs/replay.md): the run's configuration, its state, and the operator's changes
 * mid-run.
 */

import type { AssistView } from '#lib/api/fanAssist.js';
import { request } from '#lib/api/http.js';
import type { AutoDropBy, ProfileAction } from '#lib/api/profiles.js';

export type ReplayMode = 'setpoint' | 'feedforward' | 'feedforward_pid';

export type ReplayTrigger = 'profile' | 'time' | 'bt';

export type AlignOn = 'charge' | 'turning_point' | 'drying_end' | 'first_crack_start';

export type ReplayChannel = 'setpoint' | 'heater' | 'fan';

/** Where the BT loop runs (docs/control.md). */
type LoopAt = 'device' | 'server' | 'open';

export type LoopPref = 'auto' | 'device' | 'server';

export interface DeviceCaps {
	pid: boolean;
	ff: boolean;
	lease: boolean;
	tpc: boolean;
	/** An onboard RoR limiter (`control/ror_limit`, firmware 3.2 to 3.4.0). */
	ror: boolean;
	/** False when inferred from telemetry rather than reported by the device. */
	reported: boolean;
}

interface ReplayConfig {
	mode: ReplayMode;
	trigger: ReplayTrigger;
	align: AlignOn;
	fan: boolean;
	ghost: boolean;
	lookahead_seconds: number;
	/** The lookahead follows the loop's lag, up to `lookahead_seconds`. */
	adapt_lookahead?: boolean;
	hysteresis_c: number;
	loop: LoopAt;
	server_pid: { kp: number; ki: number; kd: number };
	/** A run resumed after a restart cut the last: seconds the profile waits for the lost time. */
	hold?: { seconds: number; since_elapsed: number };
}

export interface ReplayChannelState {
	channel: ReplayChannel;
	driven: boolean;
	source?: 'events' | 'curve';
	paused: boolean;
	/** Profile value now (°C for setpoint, % for heater and fan). */
	target: number | null;
	last_sent: number | null;
}

export interface ReplayEventView {
	id: string;
	action: ProfileAction;
	value: number | null;
	label: string | null;
	trigger_kind: 'time' | 'bt';
	at_seconds: number | null;
	at_bt: number | null;
	expected_t: number | null;
	/** The trigger this run uses. */
	trigger: 'time' | 'bt';
	state: 'pending' | 'waiting' | 'fired' | 'held' | 'superseded' | 'skipped';
	fire_at?: number;
	reason?: string;
	delay_seconds: number;
	eta_seconds: number | null;
	to_go_c: number | null;
}

export interface ReplayLogEntry {
	kind: string;
	channel?: ReplayChannel;
	event_id?: string;
	action?: ProfileAction;
	value?: number | null;
	label?: string | null;
	detail?: string | null;
	elapsed: number;
	live_t: number;
	profile_t: number;
}

interface ReplayAlign {
	on: AlignOn;
	applied: boolean;
	offset_seconds: number;
	offset_c: number | null;
	profile_landmark_t: number | null;
}

/** A drive's auto DROP (`ReplayState.auto_drop`). */
interface AutoDropView {
	by: AutoDropBy;
	/** Profile seconds (time), °C (bt) or seconds after first crack (development). */
	at: number;
	countdown_seconds: number;
	cancelled: boolean;
	state: 'armed' | 'counting' | 'due' | 'cancelled';
	/** Seconds to DROP: the countdown's while it runs, else an estimate. */
	eta_seconds: number | null;
	drop_at_elapsed: number | null;
	/** What it waits for before it can count down. */
	waiting_for: string | null;
	/** A ghost run shows when it would drop and marks nothing. */
	ghost: boolean;
	describe: string;
}

export interface ReplayState {
	run_id: string;
	session_id: string;
	device_id: string;
	profile_id: string;
	profile_name: string;
	active: boolean;
	config: ReplayConfig;
	lookahead_seconds: number;
	hysteresis_c: number;
	elapsed_seconds: number;
	last_sent_setpoint: number | null;
	last_sent_at: string | null;
	sends: number;
	last_error: string | null;
	stopped_reason: string | null;
	/** The run ended because the roaster cut its heater; the Roast page offers to resume. */
	stopped_by_device?: boolean;
	/** A server restart cut the run with the roast on; a resume picks up at `profile_t`. */
	stopped_by_restart?: boolean;
	telemetry_stale: boolean;
	updated_at: string;
	caps: DeviceCaps;
	loop: LoopAt;
	loop_reason: string;
	/** The device enforces a control lease for this run. */
	lease: boolean;
	/** Seconds since the session start / since CHARGE / profile time after alignment. */
	elapsed: number;
	live_t: number;
	profile_t: number;
	charge_marked: boolean;
	past_turning_point: boolean;
	align: ReplayAlign;
	/** Profile BT at the current profile time. */
	target_now: number | null;
	/** The lookahead the setpoint uses now (s): less than `lookahead_seconds` when it adapts. */
	lookahead_now?: number;
	/** Server PID output / integral, heater % (server loop only). */
	pid_output: number | null;
	pid_integral: number | null;
	/** Fan assist, while the run has it on (docs/fan-assist.md). */
	fan_assist?: AssistView | null;
	channels: ReplayChannelState[];
	upcoming: ReplayEventView[];
	recent: ReplayLogEntry[];
	/** Auto DROP, when the profile has it on (docs/replay.md). */
	auto_drop?: AutoDropView | null;
}

export interface StartReplayOptions {
	mode?: ReplayMode;
	trigger?: ReplayTrigger;
	align?: AlignOn;
	fan?: boolean;
	ghost?: boolean;
	lookahead_seconds?: number;
	/** Let the lookahead follow the loop's lag (defaults to the setting, on). */
	adapt_lookahead?: boolean;
	loop?: LoopPref;
}

export interface UpdateReplayOptions {
	ghost?: boolean;
	align?: AlignOn;
	trigger?: ReplayTrigger;
	lookahead_seconds?: number;
	/** false cancels auto DROP for this roast, true puts it back. */
	auto_drop?: boolean;
}

export const replay = {
	/** Start replaying the session's profile on the server. */
	start: (sessionId: string, opts: StartReplayOptions = {}) =>
		request<ReplayState>(
			`/api/sessions/${sessionId}/replay`,
			{ method: 'POST', body: JSON.stringify(opts) },
			true
		),

	/** Change ghost/align/trigger/lookahead mid-run; `ghost: false` engages a ghost run. */
	update: (sessionId: string, patch: UpdateReplayOptions) =>
		request<ReplayState>(
			`/api/sessions/${sessionId}/replay`,
			{ method: 'PATCH', body: JSON.stringify(patch) },
			true
		),

	stop: (sessionId: string) =>
		request<ReplayState | undefined>(
			`/api/sessions/${sessionId}/replay`,
			{ method: 'DELETE' },
			true
		),

	/** Resume after the roaster cut its heater: heater power on from the start output, then the
	 * drive with the stopped run's settings (docs/roast-sequence.md). */
	resumeAfterCut: (sessionId: string) =>
		request<ReplayState>(`/api/sessions/${sessionId}/drive/resume`, { method: 'POST' }, true),

	/** Current state, or the last run once it stopped; `active: false` with no other fields when none ran. */
	get: (sessionId: string) =>
		request<Partial<ReplayState> & { session_id: string; active: boolean }>(
			`/api/sessions/${sessionId}/replay`
		),

	/** Pause one channel, or all when `channel` is omitted. */
	pause: (sessionId: string, channel?: ReplayChannel) =>
		request<ReplayState>(
			`/api/sessions/${sessionId}/replay/pause`,
			{ method: 'POST', body: JSON.stringify({ channel }) },
			true
		),

	/** Resume one channel (or all); the profile's current value is sent at once. */
	resume: (sessionId: string, channel?: ReplayChannel) =>
		request<ReplayState>(
			`/api/sessions/${sessionId}/replay/resume`,
			{ method: 'POST', body: JSON.stringify({ channel }) },
			true
		),

	eventOp: (sessionId: string, eventId: string, op: 'skip' | 'delay' | 'fire', seconds?: number) =>
		request<ReplayState>(
			`/api/sessions/${sessionId}/replay/events/${eventId}`,
			{ method: 'POST', body: JSON.stringify({ op, seconds }) },
			true
		)
};
