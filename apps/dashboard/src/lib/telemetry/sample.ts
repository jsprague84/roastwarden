/**
 * Telemetry protocol v2 sample, as sent by `/ws/v2/telemetry` in CBOR frames
 * and by `GET /api/roaster/{id}/samples` as JSON. Mirrors `roastwarden_core::Sample`.
 * Keys are short on the wire; see the Rust struct for the meaning of each.
 */
import type { Telemetry } from '#lib/api/telemetry.js';

export interface Sample {
	/** Per-device sequence number, 1-based, monotonic. */
	q: number;
	/** Server receive time, Unix ms. */
	t: number;
	bt?: number;
	et?: number;
	/** Server RoR of BT, °C/min. */
	rb?: number;
	/** Server RoR of ET, °C/min. */
	re?: number;
	/** Device's own RoR, if any. */
	rd?: number;
	/** Heater command, %. */
	h?: number;
	/** Fan, %. */
	f?: number;
	sp?: number;
	/** 0 manual, 1 auto. */
	m?: number;
	he?: boolean;
	/** Other numeric fields keyed by their original name (rssi, freeHeap, ambientTemp…). */
	x?: Record<string, number>;
}

export interface SamplesFrame {
	type: 'samples';
	device_id: string;
	samples: Sample[];
	catchup: boolean;
	gap?: boolean;
}

/** Convert a v2 sample to the v1 `Telemetry` shape the components consume. */
export function sampleToTelemetry(s: Sample): Telemetry {
	const t: Telemetry = {
		beanTemp: s.bt ?? NaN,
		envTemp: s.et ?? NaN,
		heater: s.h ?? 0,
		fan: s.f ?? 0,
		setpoint: s.sp ?? 0,
		controlMode: s.m ?? 0,
		heaterEnable: s.he ? 1 : 0,
		timestamp: Math.floor(s.t / 1000)
	};
	if (s.rb != null) t.rateOfRise = s.rb;
	if (s.re != null) t.rateOfRiseEt = s.re;
	if (s.rd != null) t.deviceRateOfRise = s.rd;
	if (s.x) {
		if (s.x.Kp != null) t.Kp = s.x.Kp;
		if (s.x.Ki != null) t.Ki = s.x.Ki;
		if (s.x.Kd != null) t.Kd = s.x.Kd;
		if (s.x.freeHeap != null) t.freeHeap = s.x.freeHeap;
		if (s.x.rssi != null) t.rssi = s.x.rssi;
		if (s.x.systemStatus != null) t.systemStatus = s.x.systemStatus;
		if (s.x.ambientTemp != null) t.ambientTemp = s.x.ambientTemp;
		if (s.x.uptime != null) t.uptime = s.x.uptime;
		if (s.x.heaterApplied != null) t.heaterApplied = s.x.heaterApplied;
		if (s.x.rorLimited != null) t.rorLimited = s.x.rorLimited;
	}
	return t;
}

/** A sample is plottable when it carries at least one temperature. */
export function hasTemperature(s: Sample): boolean {
	return s.bt != null || s.et != null;
}
