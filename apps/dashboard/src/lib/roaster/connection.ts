/**
 * A roaster's connections in words (docs/hardware-integration-guide.md): the protocol's
 * name, a one-line summary of its settings, and the Modbus RTU serial settings the forms
 * offer. The server checks every config when it is saved; these only shape the forms.
 */
import type { ConnectionProtocol } from '#lib/api/devices.js';
import type { Help } from '#lib/constants/help.js';
import { tc4Summary } from '#lib/roaster/tc4.js';
import { phidgetSummary } from '#lib/roaster/phidget.js';

const PROTOCOL_LABEL: Record<ConnectionProtocol, string> = {
	mqtt: 'MQTT',
	websocket: 'WebSocket',
	modbus_tcp: 'Modbus TCP',
	modbus_rtu: 'Modbus RTU',
	serial_tc4: 'Serial (TC4)',
	phidget22: 'Phidgets (Phidget22 network server)'
};

export function protocolLabel(p: ConnectionProtocol): string {
	return PROTOCOL_LABEL[p] ?? p;
}

/** Modbus over TCP or RTU: both use the device's register map. */
export function isModbus(p: ConnectionProtocol): boolean {
	return p === 'modbus_tcp' || p === 'modbus_rtu';
}

export type Parity = 'none' | 'even' | 'odd';
/** How Modbus frames go over the serial line: binary RTU, or ASCII's `:`-framed hex lines. */
type SerialFraming = 'rtu' | 'ascii';
type DataBits = 7 | 8;

export const ASCII_HELP: Help = {
	line: 'Modbus ASCII: hex lines framed by ":", used by Delta DTA/DTK controllers; usually 7 data bits, even parity.',
	more: 'RTU is the usual binary framing, always 8 data bits. Pick ASCII only when the controller is set to it; the data bits, parity and stop bits must match its own settings.'
};

/** The serial framing in the usual short form, `9600 8N1` (`9600 7E1` with 7 data bits). */
export function framing(
	baud: number,
	parity: Parity,
	stopBits: number,
	dataBits: number = 8
): string {
	return `${baud} ${dataBits}${parity === 'even' ? 'E' : parity === 'odd' ? 'O' : 'N'}${stopBits}`;
}

/** One line for a connection's settings, as the roaster's Connections list shows it. */
export function connectionSummary(
	protocol: ConnectionProtocol,
	cfg: Record<string, unknown>
): string {
	if (protocol === 'mqtt') return `${cfg.topic_prefix ?? ''}`;
	if (protocol === 'websocket') return `${cfg.url ?? ''}`;
	if (protocol === 'serial_tc4') return tc4Summary(cfg);
	if (protocol === 'phidget22') return phidgetSummary(cfg);
	if (protocol === 'modbus_rtu') {
		const parity = (cfg.parity as Parity | undefined) ?? 'none';
		const ascii = cfg.framing === 'ascii';
		const bits = ascii ? Number(cfg.data_bits ?? 8) : 8;
		return `${cfg.port ?? ''} ${ascii ? 'ASCII ' : ''}${framing(Number(cfg.baud ?? 9600), parity, Number(cfg.stop_bits ?? 1), bits)} unit ${cfg.unit_id ?? 1}`;
	}
	return `${cfg.host ?? ''}:${cfg.port ?? 502}`;
}

/** Baud rates RS-485 controllers offer; 9600 is the usual default. */
export const RTU_BAUDS = [2400, 4800, 9600, 19200, 38400, 57600, 115200] as const;

export type RtuSettings = {
	port: string;
	/** RTU (binary, 8 data bits) or ASCII (Delta DTA/DTK) */
	framing: SerialFraming;
	/** Data bits; ASCII only, RTU is always 8 */
	dataBits: DataBits;
	baud: number;
	parity: Parity;
	stopBits: number;
	unitId: number;
	pollIntervalMs: number;
	/** How long a request waits for its answer, ms; left out: the server's 1000. */
	timeoutMs?: number;
};

export const RTU_DEFAULTS: RtuSettings = {
	port: '/dev/ttyUSB0',
	framing: 'rtu',
	dataBits: 8,
	baud: 9600,
	parity: 'none',
	stopBits: 1,
	unitId: 1,
	pollIntervalMs: 1000
};

/** The connection config the server reads (`ModbusRtuConnectionConfig`). RTU is 8 data bits;
 * `framing` is written only for ASCII (the server's default is RTU). */
export function rtuConfig(s: RtuSettings): Record<string, unknown> {
	const ascii = s.framing === 'ascii';
	return {
		port: s.port.trim(),
		...(ascii ? { framing: 'ascii' } : {}),
		baud: s.baud,
		parity: s.parity,
		data_bits: ascii ? s.dataBits : 8,
		stop_bits: s.stopBits,
		unit_id: s.unitId,
		poll_interval_ms: s.pollIntervalMs,
		...(s.timeoutMs ? { timeout_ms: s.timeoutMs } : {})
	};
}

/** Whether the RTU form can be saved (the server checks the same and more). */
export function rtuReady(s: RtuSettings): boolean {
	return s.port.trim() !== '' && s.baud > 0 && s.unitId >= 1 && s.unitId <= 247;
}

/** Register kinds that are read-only on every device: never writable in a map. */
export function readOnlyKind(registerType: string): boolean {
	return registerType === 'input' || registerType === 'discrete';
}

/** Coils and discrete inputs are one bit: their data type is bool. */
export function bitKind(registerType: string): boolean {
	return registerType === 'coil' || registerType === 'discrete';
}
