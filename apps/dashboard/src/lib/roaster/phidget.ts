/**
 * A Phidgets temperature board's connection (1048, TMP1101 on a VINT hub, TMP1100, 1051,
 * TMP1200 RTD), read through Phidgets' Phidget22 network server on the computer it is plugged
 * into. It reads temperatures only. The server's side is crates/server/src/transports/phidget.rs and
 * `Phidget22ConnectionConfig`; inputs are named `ch0`–`ch7`, Phidgets' channel numbers. The
 * mapping (`ingest.fields`) defaults to BT ch0, ET ch1, as Artisan reads a 1048.
 */
import type { Help } from '#lib/constants/help.js';

export type ThermocoupleType = 'J' | 'K' | 'E' | 'T';
export type RtdType = 'pt100_3850' | 'pt1000_3850' | 'pt100_3920' | 'pt1000_3920';

export const PHIDGET_PORT = 5661;
/** What the server answers in place of a saved password; sent back, it keeps the saved one. */
export const PASSWORD_MASK = '********';

export const PHIDGET_INPUTS = ['ch0', 'ch1', 'ch2', 'ch3', 'ch4', 'ch5', 'ch6', 'ch7'] as const;
export const THERMOCOUPLE_TYPES: ThermocoupleType[] = ['K', 'J', 'E', 'T'];
export const RTD_TYPES: RtdType[] = ['pt100_3850', 'pt1000_3850', 'pt100_3920', 'pt1000_3920'];
export const RTD_LABEL: Record<RtdType, string> = {
	pt100_3850: 'Pt100, α 0.00385',
	pt1000_3850: 'Pt1000, α 0.00385',
	pt100_3920: 'Pt100, α 0.00392',
	pt1000_3920: 'Pt1000, α 0.00392'
};

export const INPUT_HELP: Help = {
	line: "The board's temperature input: ch0 is the first.",
	more: 'Phidgets number the inputs from 0. On a 1048 or a TMP1101, ch0 to ch3 are the thermocouples and ch4 the board’s own sensor. Artisan reads BT on ch0 and ET on ch1.'
};

export interface PhidgetSettings {
	host: string;
	port: number;
	/** Typed here; '' with `passwordSaved` keeps the saved one, '' without it: none. */
	password: string;
	/** The saved connection has a password (the server shows it as `********`). */
	passwordSaved: boolean;
	/** The board's (a VINT board's is its hub's); null: the first temperature board. */
	serialNumber: number | null;
	/** A VINT board's hub port, 0–5; null: any. */
	hubPort: number | null;
	pollIntervalMs: number;
	dataIntervalMs: number;
	thermocoupleType: ThermocoupleType;
	rtdType: RtdType;
	rtdWires: number;
	/** The input read as BT, `ch0`; '' for none. */
	bt: string;
	/** The input read as ET, `ch1`; '' for none. */
	et: string;
	/** The saved mapping's fields (extra channels, a scale or offset), kept as they were. */
	fields: Record<string, unknown>;
	/** Per-input thermocouple types the form does not show, kept as they were. */
	thermocoupleTypes: Record<string, ThermocoupleType> | null;
}

export function defaultPhidgetSettings(host = '', port = PHIDGET_PORT): PhidgetSettings {
	return {
		host,
		port,
		password: '',
		passwordSaved: false,
		serialNumber: null,
		hubPort: null,
		pollIntervalMs: 1000,
		dataIntervalMs: 256,
		thermocoupleType: 'K',
		rtdType: 'pt100_3850',
		rtdWires: 2,
		bt: 'ch0',
		et: 'ch1',
		fields: {},
		thermocoupleTypes: null
	};
}

const obj = (v: unknown): Record<string, unknown> | null =>
	v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null;

/** A mapped field's input: `"ch0"` or `{path: "ch0", …}`. */
function pathOf(spec: unknown): string {
	if (typeof spec === 'string') return spec;
	const o = obj(spec);
	return typeof o?.path === 'string' ? o.path : '';
}

/** Whether `s` names a board input, `ch0` to `ch7`. */
export function isInput(s: string): boolean {
	return (PHIDGET_INPUTS as readonly string[]).includes(s.trim());
}

/** A config as the form's settings, reading a missing setting as the server does. */
export function phidgetSettingsOf(cfg: Record<string, unknown>): PhidgetSettings {
	const d = defaultPhidgetSettings();
	const n = (v: unknown, def: number) => (typeof v === 'number' && Number.isFinite(v) ? v : def);
	const opt = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : null);
	const fields = obj(obj(cfg.ingest)?.fields);
	const saved = typeof cfg.password === 'string' && cfg.password !== '';
	const types = obj(cfg.thermocouple_types);
	return {
		host: String(cfg.host ?? ''),
		port: n(cfg.port, PHIDGET_PORT),
		password: saved && cfg.password !== PASSWORD_MASK ? String(cfg.password) : '',
		passwordSaved: cfg.password === PASSWORD_MASK,
		serialNumber: opt(cfg.serial_number),
		hubPort: opt(cfg.hub_port),
		pollIntervalMs: n(cfg.poll_interval_ms, d.pollIntervalMs),
		dataIntervalMs: n(cfg.data_interval_ms, d.dataIntervalMs),
		thermocoupleType: THERMOCOUPLE_TYPES.includes(cfg.thermocouple_type as ThermocoupleType)
			? (cfg.thermocouple_type as ThermocoupleType)
			: 'K',
		rtdType: RTD_TYPES.includes(cfg.rtd_type as RtdType) ? (cfg.rtd_type as RtdType) : 'pt100_3850',
		rtdWires: n(cfg.rtd_wires, 2),
		// No mapping: the server reads BT ch0, ET ch1
		bt: fields ? pathOf(fields.bt) : 'ch0',
		et: fields ? pathOf(fields.et) : 'ch1',
		fields: fields ? { ...fields } : {},
		thermocoupleTypes:
			types && Object.keys(types).length ? ({ ...types } as Record<string, ThermocoupleType>) : null
	};
}

/** A role's field with its input changed; a scale or offset it had is kept. */
function withPath(spec: unknown, path: string): unknown {
	const o = obj(spec);
	return o ? { ...o, path } : path;
}

/** The mapping the settings describe: the saved one with BT and ET from the form. */
export function phidgetFields(s: PhidgetSettings): Record<string, unknown> {
	const out: Record<string, unknown> = { ...s.fields };
	for (const role of ['bt', 'et'] as const) {
		const input = s[role].trim();
		if (input) out[role] = withPath(out[role], input);
		else delete out[role];
	}
	return out;
}

/** The connection's JSON config, as the server stores it. The default mapping (BT ch0, ET ch1)
 * is written out: the config says what it reads. */
export function phidgetConfig(s: PhidgetSettings): Record<string, unknown> {
	const password = s.password !== '' ? s.password : s.passwordSaved ? PASSWORD_MASK : '';
	return {
		host: s.host.trim(),
		port: s.port,
		...(password ? { password } : {}),
		...(s.serialNumber != null ? { serial_number: s.serialNumber } : {}),
		...(s.hubPort != null ? { hub_port: s.hubPort } : {}),
		poll_interval_ms: s.pollIntervalMs,
		data_interval_ms: s.dataIntervalMs,
		thermocouple_type: s.thermocoupleType,
		...(s.thermocoupleTypes ? { thermocouple_types: { ...s.thermocoupleTypes } } : {}),
		rtd_type: s.rtdType,
		rtd_wires: s.rtdWires,
		ingest: { fields: phidgetFields(s) }
	};
}

/** What is wrong with the settings, in words; empty when they can be saved (the server checks
 * the same and more). */
export function phidgetProblems(s: PhidgetSettings): string[] {
	const out: string[] = [];
	const host = s.host.trim();
	if (host === '' || /\s/.test(host))
		out.push('Give the computer the board is plugged into (its Phidget22 network server).');
	if (!(Number.isInteger(s.port) && s.port > 0 && s.port <= 65535))
		out.push('The port is 1 to 65535 (the network server’s is 5661).');
	if (!s.bt.trim() && !s.et.trim()) out.push('Pick the input BT (or ET) is on.');
	for (const role of ['bt', 'et'] as const) {
		const v = s[role].trim();
		if (v && !isInput(v))
			out.push(`${role.toUpperCase()}: '${v}' is not a board input (ch0 to ch7).`);
	}
	if (s.bt.trim() && s.bt.trim() === s.et.trim()) out.push('BT and ET are on the same input.');
	if (s.serialNumber != null && !(Number.isInteger(s.serialNumber) && s.serialNumber > 0))
		out.push('The serial number is a whole number (empty: the first board).');
	if (s.hubPort != null && !(Number.isInteger(s.hubPort) && s.hubPort >= 0 && s.hubPort <= 5))
		out.push('The hub port is 0 to 5.');
	if (!(s.dataIntervalMs >= 8 && s.dataIntervalMs <= 60000))
		out.push('The data interval is 8 to 60000 ms.');
	if (!(s.pollIntervalMs >= 100)) out.push('The poll is at least 100 ms.');
	if (![2, 3, 4].includes(s.rtdWires)) out.push('An RTD has 2, 3 or 4 wires.');
	if (s.password.length > 64) out.push('The password is at most 64 characters.');
	return out;
}

/** BT and ET's inputs in a line: `BT ch0 · ET ch1`. */
function rolesLine(fields: Record<string, unknown> | null): string {
	const f = fields ?? { bt: 'ch0', et: 'ch1' };
	const parts = (['bt', 'et'] as const)
		.filter((r) => pathOf(f[r]))
		.map((r) => `${r.toUpperCase()} ${pathOf(f[r])}`);
	const extra = Object.keys(f).filter((k) => k !== 'bt' && k !== 'et').length;
	if (extra) parts.push(`+${extra}`);
	return parts.join(' · ');
}

/** The board the settings pick, in a line: `board 370001 port 2`; '' for the first one. */
export function boardLine(s: Pick<PhidgetSettings, 'serialNumber' | 'hubPort'>): string {
	const parts: string[] = [];
	if (s.serialNumber != null) parts.push(`board ${s.serialNumber}`);
	if (s.hubPort != null) parts.push(`hub port ${s.hubPort}`);
	return parts.join(' ');
}

/** One line for a saved connection: `phidgetsbc.local:5661 · BT ch0 · ET ch1 · type K`. */
export function phidgetSummary(cfg: Record<string, unknown>): string {
	const s = phidgetSettingsOf(cfg);
	const board = boardLine(s);
	return [
		`${s.host}:${s.port}`,
		rolesLine(obj(obj(cfg.ingest)?.fields)),
		`type ${s.thermocoupleType}`,
		board
	]
		.filter(Boolean)
		.join(' · ');
}
