/**
 * Artisan's machine setups in the Add roaster wizard (docs/artisan-machines.md): the catalogue
 * filtered by what you type, and a plan from the server turned into the wizard's fields. The
 * server reads the `.aset` and plans the roaster (Modbus RTU or ASCII, Modbus TCP, a TC4, a
 * Phidgets board, a WebSocket or MQTT); this only fills the form, which you check and the normal calls save
 * (control stays off). What the form does not show (a mapping, a poll, command templates)
 * rides along in the plan's config.
 */
import type { ArtisanMachine, ArtisanPlan, ConnectionProtocol } from '#lib/api/devices.js';
import type { RoasterChannel, RoasterKind, RoasterOutputs } from '#lib/api/roasters.js';
import { RTU_DEFAULTS, type Parity, type RtuSettings } from '#lib/roaster/connection.js';
import { draftOf, type RegisterDraft } from '#lib/roaster/registerRoles.js';
import { tc4SettingsOf, type Tc4Settings } from '#lib/roaster/tc4.js';
import { phidgetSettingsOf, type PhidgetSettings } from '#lib/roaster/phidget.js';

/** The machines matching every word of `query` (brand, model, name or Artisan device), the
 * supported ones only unless `all`. */
export function filterMachines(
	list: ArtisanMachine[],
	query: string,
	all: boolean
): ArtisanMachine[] {
	const words = query.toLowerCase().split(/\s+/).filter(Boolean);
	return list.filter((m) => {
		if (!all && !m.supported) return false;
		const hay = `${m.key} ${m.brand} ${m.model} ${m.name} ${m.artisan_device_name}`
			.toLowerCase()
			.replace(/_/g, ' ');
		return words.every((w) => hay.includes(w));
	});
}

/** A roaster id from a machine: `Berto/Autonics_Control` → `berto_autonics_control`. */
export function suggestedId(m: ArtisanMachine): string {
	return m.key
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '_')
		.replace(/^_+|_+$/g, '')
		.slice(0, 48);
}

/** The Modbus TCP fields of the wizard. */
export interface TcpSettings {
	host: string;
	port: number;
	unitId: number;
	pollIntervalMs: number;
	timeoutMs?: number;
}

/** A plan as the wizard's fields. */
export interface ArtisanPrefill {
	machine: ArtisanMachine;
	source: string;
	name: string;
	protocol: ConnectionProtocol;
	/** The Modbus TCP fields, for a TCP machine */
	tcp: TcpSettings | null;
	/** The Modbus RTU (or ASCII) fields, for a serial machine (its port empty: the server's to name) */
	rtu: RtuSettings | null;
	/** The TC4 fields, for an Arduino TC4 machine (its port empty: the server's to name) */
	tc4: Tc4Settings | null;
	/** The Phidgets board's fields (its host empty when the plan needs it: the owner's to type) */
	phidget: PhidgetSettings | null;
	/** The WebSocket's url and reconnect wait; the url keeps `127.0.0.1` when the host is needed */
	ws: { url: string; reconnectMs: number } | null;
	/** The MQTT fields: the topic the roaster's data arrives on (empty when needed) and QoS */
	mqtt: { telemetryTopic: string; qos: number } | null;
	/** The plan's whole connection config: the form's fields go over it when it is saved */
	extra: Record<string, unknown>;
	registers: RegisterDraft[];
	kind: RoasterKind;
	outputs: RoasterOutputs;
	fan_floor_pct: number;
	channels: RoasterChannel[];
	/** What the file cannot know: `host`, `port`, `telemetry_topic` */
	needs: string[];
	notes: string[];
}

const num = (v: unknown, d: number) => (typeof v === 'number' && Number.isFinite(v) ? v : d);
const parity = (v: unknown): Parity => (v === 'even' || v === 'odd' ? v : 'none');

/** The wizard's fields from a plan; null when Roastwarden cannot set the machine up. */
export function prefillFromPlan(plan: ArtisanPlan): ArtisanPrefill | null {
	const { protocol, config: c, setup } = plan;
	if (!plan.machine.supported || !c || !setup || !protocol) return null;
	const needs = plan.needs ?? [];
	const pollIntervalMs = num(c.poll_interval_ms, 1000);
	const timeoutMs = typeof c.timeout_ms === 'number' ? c.timeout_ms : undefined;
	const unitId = num(c.unit_id, 1);
	const ascii = c.framing === 'ascii';
	return {
		machine: plan.machine,
		source: plan.source,
		name: plan.machine.name,
		protocol,
		tcp:
			protocol === 'modbus_tcp'
				? {
						host: needs.includes('host') ? '' : String(c.host ?? ''),
						port: num(c.port, 502),
						unitId,
						pollIntervalMs,
						timeoutMs
					}
				: null,
		rtu:
			protocol === 'modbus_rtu'
				? {
						...RTU_DEFAULTS,
						port: needs.includes('port') ? '' : String(c.port ?? ''),
						framing: ascii ? 'ascii' : 'rtu',
						dataBits: ascii && c.data_bits === 7 ? 7 : 8,
						baud: num(c.baud, RTU_DEFAULTS.baud),
						parity: parity(c.parity),
						stopBits: num(c.stop_bits, 1),
						unitId,
						pollIntervalMs,
						timeoutMs
					}
				: null,
		tc4:
			protocol === 'serial_tc4'
				? { ...tc4SettingsOf(c), port: needs.includes('port') ? '' : String(c.port ?? '') }
				: null,
		phidget:
			protocol === 'phidget22'
				? { ...phidgetSettingsOf(c), host: needs.includes('host') ? '' : String(c.host ?? '') }
				: null,
		ws:
			protocol === 'websocket'
				? { url: String(c.url ?? 'ws://'), reconnectMs: num(c.reconnect_interval_ms, 5000) }
				: null,
		mqtt:
			protocol === 'mqtt'
				? {
						telemetryTopic: needs.includes('telemetry_topic')
							? ''
							: String(c.telemetry_topic ?? ''),
						qos: num(c.qos, 0)
					}
				: null,
		extra: c,
		registers: plan.registers.map((r) => draftOf(r as RegisterDraft)),
		kind: setup.kind,
		outputs: setup.outputs,
		fan_floor_pct: setup.fan_floor_pct,
		channels: setup.channels,
		needs,
		notes: plan.notes
	};
}

/** The connection config to save: the form's fields over the plan's config when the form is
 * still on the plan's protocol (its mapping, poll and commands kept); the form's alone otherwise.
 * A Modbus form shows its whole config (the map is separate), so a field it leaves out, such as
 * ASCII framing switched back to RTU, must not come back from the plan; a Phidget form carries
 * its whole config too (its mapping kept), so a board or hub port cleared stays cleared. */
export function connectionConfigFromPlan(
	p: Pick<ArtisanPrefill, 'protocol' | 'extra'> | null,
	protocol: ConnectionProtocol,
	form: Record<string, unknown>
): Record<string, unknown> {
	if (
		!p ||
		p.protocol !== protocol ||
		protocol === 'modbus_tcp' ||
		protocol === 'modbus_rtu' ||
		protocol === 'phidget22'
	)
		return form;
	return { ...p.extra, ...form };
}

/** A need of the plan in words, for "Next step: give the …". */
export function needLabel(need: string): string {
	if (need === 'host') return "roaster's address";
	if (need === 'port') return 'serial port';
	if (need === 'telemetry_topic') return 'MQTT topic its data arrives on';
	return need;
}

/** Whether a url still points at the placeholder host a plan leaves when it needs the address. */
export function placeholderHost(url: string): boolean {
	return /\/\/(127\.0\.0\.1|localhost)([:/]|$)/i.test(url.trim());
}

/** What a plan's config brings that the form does not show, in a line:
 * `9 mapped fields, the getCurrentRoastingStep poll, 3 command templates`; '' when nothing. */
export function planExtras(config: Record<string, unknown>): string {
	const parts: string[] = [];
	const obj = (v: unknown) =>
		v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
	const ingest = obj(config.ingest);
	const fields = obj(ingest?.fields);
	if (fields) {
		const n = Object.keys(fields).length;
		if (n)
			parts.push(`${n} mapped field${n === 1 ? '' : 's'}${ingest?.units === 'F' ? ' in °F' : ''}`);
	}
	const poll = obj(config.poll);
	if (poll) {
		const cmd = /"command"\s*:\s*"([^"]+)"/.exec(String(poll.message ?? ''))?.[1];
		parts.push(cmd ? `the ${cmd} poll` : 'a poll');
	}
	const commands = obj(config.commands);
	if (commands) {
		const n = Object.keys(commands).length;
		if (n) parts.push(`${n} command template${n === 1 ? '' : 's'}`);
	}
	return parts.join(', ');
}

/** The outputs and channels a prefill declares, in a line: `Burner, Air; channels IT, DT`. */
export function declaredSummary(p: Pick<ArtisanPrefill, 'outputs' | 'channels'>): string {
	const outs = (['heater', 'fan', 'drum', 'damper'] as const)
		.map((k) => p.outputs[k]?.label)
		.filter((l): l is string => !!l);
	const parts = [outs.length ? `outputs ${outs.join(', ')}` : 'no outputs (records only)'];
	if (p.channels.length) parts.push(`channels ${p.channels.map((c) => c.label).join(', ')}`);
	return parts.join('; ');
}
