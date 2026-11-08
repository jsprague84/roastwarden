/**
 * Adding a roaster (docs/add-roaster.md): its raw messages, a mapping's preview, the check against
 * the interface standard, the owner's write check.
 */

import { request, roasterPath } from '#lib/api/http.js';

type RawProtocol = 'mqtt' | 'websocket' | 'serial_tc4' | 'modbus_tcp' | 'modbus_rtu' | 'phidget22';

/** The newest message a roaster sent on one connection and topic, before any mapping. */
export interface RawMessage {
	connection_id: string | null;
	protocol: RawProtocol;
	topic: string | null;
	received_at: string;
	/** JSON text, a TC4 line, or (Modbus) a JSON list of decoded registers; at most 4 KB. */
	payload: string;
	truncated: boolean;
}

/** One mapped field in a preview: what it read and what it became, or why not. */
interface PreviewField {
	key: string;
	field: string;
	source: string;
	raw: unknown;
	value: number | null;
	problem: string | null;
}

export interface IngestPreview {
	connection_id: string;
	protocol: RawProtocol;
	messages: RawMessage[];
	/** The sample the mapping makes (field → value, °C), or null. */
	sample: Record<string, number> | null;
	fields: PreviewField[];
	note: string | null;
}

export type CheckStatus = 'ok' | 'warn' | 'fail';

export interface CheckLine {
	check: string;
	status: CheckStatus;
	message: string;
}

/** A roaster against the interface standard (`GET …/check`, read-only). */
export interface RoasterCheck {
	device_id: string;
	verdict: CheckStatus;
	lines: CheckLine[];
	rate_hz: number | null;
	samples: number;
	reading: Record<string, number> | null;
}

interface WriteCheckStep {
	step: string;
	ok: boolean;
	at_ms: number;
	message: string;
}

/** The owner's write check: heater 10 %, its echo, emergency stop, the heater back at 0. */
export interface WriteCheck {
	device_id: string;
	ok: boolean;
	steps: WriteCheckStep[];
}

export const roasterCheck = {
	raw: (deviceId: string) => request<RawMessage[]>(`${roasterPath(deviceId)}/raw`),
	preview: (deviceId: string, connectionId: string, ingest: unknown) =>
		request<IngestPreview>(
			`${roasterPath(deviceId)}/ingest/preview`,
			{ method: 'POST', body: JSON.stringify({ connection_id: connectionId, ingest }) },
			true
		),
	get: (deviceId: string) => request<RoasterCheck>(`${roasterPath(deviceId)}/check`),
	/** Owner key only; sends heater 10 % and always ends with emergency stop. */
	write: (deviceId: string) =>
		request<WriteCheck>(`${roasterPath(deviceId)}/check/write`, { method: 'POST' }, true),
	/** Save a connection's config (the mapping), with the server's own words on a 400. */
	saveConfig: (registryId: string, connectionId: string, config: Record<string, unknown>) =>
		request<unknown>(
			`/api/devices/${encodeURIComponent(registryId)}/connections/${encodeURIComponent(connectionId)}`,
			{ method: 'PUT', body: JSON.stringify({ config }) },
			true
		)
};
