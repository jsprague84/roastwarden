/**
 * Roasters as records (devices): their connections, templates (device profiles) and Modbus
 * register maps, a connection tested, and a roaster planned from an Artisan machine setup
 * (docs/artisan-machines.md).
 */

import type {
	ControlLevel,
	RoasterChannel,
	RoasterKind,
	RoasterOutputs
} from '#lib/api/roasters.js';

export type ConfiguredDeviceStatus = 'pending' | 'active' | 'disabled' | 'error';

export type ConnectionProtocol =
	'mqtt' | 'websocket' | 'modbus_tcp' | 'modbus_rtu' | 'serial_tc4' | 'phidget22';

export type ConfiguredDevice = {
	id: string;
	name: string;
	device_id: string;
	profile_id?: string;
	status: ConfiguredDeviceStatus;
	description?: string;
	location?: string;
	created_at: string;
	updated_at: string;
	last_seen_at?: string;
};

export type DeviceConnection = {
	id: string;
	device_id: string;
	protocol: ConnectionProtocol;
	enabled: boolean;
	priority: number;
	config: Record<string, unknown>;
	created_at: string;
	updated_at: string;
};

export type ConfiguredDeviceWithConnections = ConfiguredDevice & {
	connections: DeviceConnection[];
};

export type CreateDeviceRequest = {
	device_id: string;
	name: string;
	profile_id?: string;
	description?: string;
	location?: string;
};

export type UpdateDeviceRequest = {
	name?: string;
	profile_id?: string;
	status?: ConfiguredDeviceStatus;
	description?: string;
	location?: string;
};

export type DeviceProfile = {
	id: string;
	name: string;
	description?: string;
	default_control_mode?: string;
	default_setpoint?: number;
	/** Fan, %. */
	default_fan?: number;
	default_kp?: number;
	default_ki?: number;
	default_kd?: number;
	max_temp?: number;
	/** % */
	min_fan?: number;
	telemetry_interval_ms?: number;
	created_at: string;
	updated_at: string;
};

export type CreateDeviceProfileRequest = {
	name: string;
	description?: string;
	default_control_mode?: string;
	default_setpoint?: number;
	/** Fan, %. */
	default_fan?: number;
	default_kp?: number;
	default_ki?: number;
	default_kd?: number;
	max_temp?: number;
	/** % */
	min_fan?: number;
	telemetry_interval_ms?: number;
};

// Connection testing types
export type TestConnectionRequest = {
	protocol: ConnectionProtocol;
	config: Record<string, unknown>;
	device_id?: string;
};

export type TestConnectionResponse = {
	success: boolean;
	message: string;
	latency_ms?: number;
};

export type CreateConnectionRequest = {
	protocol: ConnectionProtocol;
	enabled: boolean;
	priority?: number;
	config: Record<string, unknown>;
};

export type UpdateConnectionRequest = {
	enabled?: boolean;
	priority?: number;
	config?: Record<string, unknown>;
};

// Modbus register map types
export type ModbusRegisterMapEntry = {
	id: string;
	device_id: string;
	register_type: string;
	address: number;
	name: string;
	/** uint16, int16, uint32, int32, float32, bcd16, bcd32 (two registers, in the map's word order) or bool */
	data_type: string;
	byte_order: string;
	scale_factor: number;
	offset: number;
	unit?: string;
	description?: string;
	writable: boolean;
	/** What the register is (lib/roaster/registerRoles.ts); null: by its name. */
	role?: string | null;
	/** The register's own Modbus slave, 1–247 (a bus of several controllers); null: the connection's. */
	unit_id?: number | null;
};

export type CreateRegisterMapEntry = {
	register_type: string;
	address: number;
	name: string;
	/** uint16, int16, uint32, int32, float32, bcd16, bcd32 (two registers, in the map's word order) or bool */
	data_type: string;
	byte_order?: string;
	scale_factor?: number;
	offset?: number;
	unit?: string;
	description?: string;
	writable?: boolean;
	/** What the register is (lib/roaster/registerRoles.ts); null: by its name. */
	role?: string | null;
	/** The register's own Modbus slave, 1–247 (a bus of several controllers); null: the connection's. */
	unit_id?: number | null;
};

/** One machine in Artisan's catalogue (`GET /api/artisan/machines`). */
export type ArtisanMachine = {
	/** `Brand/Model`, as Artisan's file is named */
	key: string;
	brand: string;
	model: string;
	name: string;
	artisan_device: number;
	artisan_device_name: string;
	/** Roastwarden can set it up from this file */
	supported: boolean;
	/** Why not, when it cannot */
	why?: string;
};

/** The setup a plan proposes (always control off). */
type ArtisanPlanSetup = {
	kind: RoasterKind;
	control: ControlLevel;
	outputs: RoasterOutputs;
	fan_floor_pct: number;
	channels: RoasterChannel[];
};

/** A roaster planned from an Artisan machine setup (`POST /api/artisan/machines/plan`); nothing is saved. */
export type ArtisanPlan = {
	machine: ArtisanMachine;
	/** Artisan's catalogue at its commit, or the file sent */
	source: string;
	/** Any protocol: Modbus RTU (or ASCII) and TCP, a serial TC4, a WebSocket, MQTT */
	protocol?: ConnectionProtocol;
	/** The connection config the server takes for that protocol; a WebSocket's or MQTT's carries
	 * its `ingest` mapping, a poll and `commands` templates the wizard's form does not show */
	config?: Record<string, unknown>;
	/** The register map, for Modbus only */
	registers: CreateRegisterMapEntry[];
	setup?: ArtisanPlanSetup;
	/** What the file cannot know: `host` (Modbus TCP, or a WebSocket url left at 127.0.0.1),
	 * `port` (the serial port on the server), `telemetry_topic` (the MQTT topic its data arrives on) */
	needs: string[];
	notes: string[];
};

/** A catalogue machine, or a setup file's text. */
export type PlanArtisanRequest = { machine: string } | { aset: string; file_name?: string };

const BASE = ''; // proxied in dev via Vite, same-origin in prod

async function j<T>(url: string, init?: RequestInit): Promise<T> {
	// The browser's login cookie goes with it (same origin)
	const r = await fetch(BASE + url, { ...init, credentials: 'same-origin' });
	if (r.status === 401) {
		window.dispatchEvent(new CustomEvent('auth_required'));
		throw new Error('Authentication required');
	}
	if (!r.ok) throw new Error(`${r.status} ${r.statusText}`);
	if (r.headers.get('content-type')?.includes('application/json')) return r.json();
	return undefined as T;
}

const jsonHeaders = { 'content-type': 'application/json' } as const;

// --- Device CRUD ---

export function listDevices(status?: ConfiguredDeviceStatus): Promise<ConfiguredDevice[]> {
	const params = status ? `?status=${status}` : '';
	return j<ConfiguredDevice[]>(`/api/devices${params}`);
}

export function getDevice(id: string): Promise<ConfiguredDeviceWithConnections> {
	return j<ConfiguredDeviceWithConnections>(`/api/devices/${encodeURIComponent(id)}`);
}

export function createDevice(req: CreateDeviceRequest): Promise<ConfiguredDevice> {
	return j<ConfiguredDevice>(`/api/devices`, {
		method: 'POST',
		headers: jsonHeaders,
		body: JSON.stringify(req)
	});
}

export function updateDevice(id: string, req: UpdateDeviceRequest): Promise<ConfiguredDevice> {
	return j<ConfiguredDevice>(`/api/devices/${encodeURIComponent(id)}`, {
		method: 'PUT',
		headers: jsonHeaders,
		body: JSON.stringify(req)
	});
}

export function deleteDevice(id: string): Promise<void> {
	return j<void>(`/api/devices/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

// --- Discovered devices ---

export function getDiscoveredDevices(): Promise<ConfiguredDevice[]> {
	return j<ConfiguredDevice[]>(`/api/devices/discovered`);
}

// --- Device Profiles ---

export function createDeviceProfile(req: CreateDeviceProfileRequest): Promise<DeviceProfile> {
	return j<DeviceProfile>(`/api/device-profiles`, {
		method: 'POST',
		headers: jsonHeaders,
		body: JSON.stringify(req)
	});
}

export function deleteDeviceProfile(id: string): Promise<void> {
	return j<void>(`/api/device-profiles/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

// --- Connections ---

export function addConnection(
	deviceId: string,
	req: CreateConnectionRequest
): Promise<DeviceConnection> {
	return j<DeviceConnection>(`/api/devices/${encodeURIComponent(deviceId)}/connections`, {
		method: 'POST',
		headers: jsonHeaders,
		body: JSON.stringify(req)
	});
}

export function updateConnection(
	deviceId: string,
	connId: string,
	req: UpdateConnectionRequest
): Promise<DeviceConnection> {
	return j<DeviceConnection>(
		`/api/devices/${encodeURIComponent(deviceId)}/connections/${encodeURIComponent(connId)}`,
		{
			method: 'PUT',
			headers: jsonHeaders,
			body: JSON.stringify(req)
		}
	);
}

export function removeConnection(deviceId: string, connId: string): Promise<void> {
	return j<void>(
		`/api/devices/${encodeURIComponent(deviceId)}/connections/${encodeURIComponent(connId)}`,
		{
			method: 'DELETE'
		}
	);
}

// --- Register Map ---

export function getRegisterMap(deviceId: string): Promise<ModbusRegisterMapEntry[]> {
	return j<ModbusRegisterMapEntry[]>(`/api/devices/${encodeURIComponent(deviceId)}/register-map`);
}

export function setRegisterMap(
	deviceId: string,
	registers: CreateRegisterMapEntry[]
): Promise<unknown> {
	return j<unknown>(`/api/devices/${encodeURIComponent(deviceId)}/register-map`, {
		method: 'PUT',
		headers: jsonHeaders,
		body: JSON.stringify(registers)
	});
}

// --- Connection Testing ---

export function testConnection(req: TestConnectionRequest): Promise<TestConnectionResponse> {
	return j<TestConnectionResponse>(`/api/devices/test-connection`, {
		method: 'POST',
		headers: jsonHeaders,
		body: JSON.stringify(req)
	});
}

// --- Artisan's machine setups (docs/artisan-machines.md) ---

/** Artisan's machine catalogue: each machine and whether Roastwarden can set it up. */
export function listArtisanMachines(): Promise<ArtisanMachine[]> {
	return j<ArtisanMachine[]>(`/api/artisan/machines`);
}

/** A roaster planned from a catalogue machine or an `.aset` file's text; nothing is saved. */
export async function planArtisanMachine(req: PlanArtisanRequest): Promise<ArtisanPlan> {
	const r = await fetch(BASE + `/api/artisan/machines/plan`, {
		method: 'POST',
		headers: jsonHeaders,
		body: JSON.stringify(req),
		credentials: 'same-origin'
	});
	if (r.status === 401) {
		window.dispatchEvent(new CustomEvent('auth_required'));
		throw new Error('Authentication required');
	}
	// The server says why (not an Artisan setup, no such machine): pass its words on
	if (!r.ok) {
		const body = await r.json().catch(() => null);
		throw new Error(body?.error?.message ?? `${r.status} ${r.statusText}`);
	}
	return r.json();
}
