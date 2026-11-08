/**
 * One connection form for the Add roaster wizard and a roaster's Add connection: the
 * fields of every protocol as one draft, the protocols in plain words, what the form still
 * needs, and the config the server stores. The server checks every config when it is saved;
 * this only shapes the form.
 */
import type { ConnectionProtocol } from '#lib/api/devices.js';
import type { TcpSettings } from '#lib/roaster/artisan.js';
import { RTU_DEFAULTS, rtuConfig, rtuReady, type RtuSettings } from '#lib/roaster/connection.js';
import { defaultTc4Settings, tc4Config, tc4Problems, type Tc4Settings } from '#lib/roaster/tc4.js';
import {
	defaultPhidgetSettings,
	PHIDGET_PORT,
	phidgetConfig,
	phidgetProblems,
	type PhidgetSettings
} from '#lib/roaster/phidget.js';

export interface ConnectionDraft {
	protocol: ConnectionProtocol;
	/** MQTT: the prefix its commands go under and its data comes from; QoS; a topic of its own
	 * for the data when it is not `<prefix>/telemetry` (a bridged broker), '' for none */
	mqtt: { topicPrefix: string; telemetryTopic: string; qos: number };
	ws: { url: string; reconnectMs: number };
	tcp: TcpSettings;
	rtu: RtuSettings;
	tc4: Tc4Settings;
	phidget: PhidgetSettings;
}

/** The protocols in the order a home roaster meets them, in plain words. */
export const PROTOCOL_CHOICES: { protocol: ConnectionProtocol; title: string; line: string }[] = [
	{
		protocol: 'mqtt',
		title: 'Wi-Fi (MQTT)',
		line: "Sends its readings to the server's broker: Roastwarden's ESP32 firmware, Tasmota, ESPHome."
	},
	{
		protocol: 'serial_tc4',
		title: 'USB thermocouple board (TC4)',
		line: 'A TC4 or aArtisan board on a USB port of the server.'
	},
	{
		protocol: 'phidget22',
		title: 'Phidgets temperature board',
		line: "A 1048, TMP1101 or other Phidgets board, through Phidgets' network server on the computer it is plugged into."
	},
	{
		protocol: 'modbus_rtu',
		title: 'Controller on RS-485 (Modbus RTU)',
		line: "A PID controller or a roaster's PLC on a serial adapter of the server."
	},
	{
		protocol: 'modbus_tcp',
		title: 'Controller on the network (Modbus TCP)',
		line: "A roaster's PLC or a Modbus gateway on the LAN."
	},
	{
		protocol: 'websocket',
		title: 'WebSocket',
		line: "A roaster that serves Artisan's WebSocket protocol."
	}
];

/** The guide for people who program their own roaster (docs/connect-your-roaster.md). */
export const CONNECT_GUIDE = 'docs/connect-your-roaster.md';

/** The guide in the repository at the running revision (About's `source_url`), or null. */
export function connectGuideUrl(sourceUrl: string | null | undefined): string | null {
	const base = sourceUrl?.trim().replace(/\/+$/, '');
	return base ? `${base}/${CONNECT_GUIDE}` : null;
}

/** The MQTT prefix the server would use for this roaster (`roaster/<id>`). */
export function mqttPrefixFor(deviceId: string): string {
	const id = deviceId.trim();
	return id ? `roaster/${id}` : 'roaster/';
}

/** Where a roaster found on the network starts: its protocol, host and port. */
export interface FoundAt {
	protocol?: string | null;
	host?: string | null;
	port?: string | number | null;
}

const PROTOCOLS: ConnectionProtocol[] = [
	'mqtt',
	'websocket',
	'modbus_tcp',
	'modbus_rtu',
	'serial_tc4',
	'phidget22'
];

export function isProtocol(p: unknown): p is ConnectionProtocol {
	return typeof p === 'string' && (PROTOCOLS as string[]).includes(p);
}

/** A fresh form, MQTT first, or prefilled from a roaster found on the network. */
export function newDraft(deviceId = '', found: FoundAt = {}): ConnectionDraft {
	const protocol = isProtocol(found.protocol) ? found.protocol : 'mqtt';
	const host = found.host ?? '';
	const port = Number(found.port);
	const foundPort = Number.isFinite(port) && port > 0 ? port : 0;
	return {
		protocol,
		mqtt: { topicPrefix: mqttPrefixFor(deviceId), telemetryTopic: '', qos: 0 },
		ws: {
			url: protocol === 'websocket' && host ? `ws://${host}:${foundPort || 80}/` : 'ws://',
			reconnectMs: 5000
		},
		tcp: {
			host: protocol === 'modbus_tcp' ? host : '',
			port: protocol === 'modbus_tcp' && foundPort ? foundPort : 502,
			unitId: 1,
			pollIntervalMs: 1000
		},
		rtu: { ...RTU_DEFAULTS },
		tc4: defaultTc4Settings(),
		phidget: defaultPhidgetSettings(
			protocol === 'phidget22' ? host : '',
			(protocol === 'phidget22' && foundPort) || PHIDGET_PORT
		)
	};
}

/** The prefix follows the roaster's id until someone edits it. */
export function followId(d: ConnectionDraft, oldId: string, newId: string): void {
	if (d.mqtt.topicPrefix === mqttPrefixFor(oldId) || d.mqtt.topicPrefix.trim() === '')
		d.mqtt.topicPrefix = mqttPrefixFor(newId);
}

/** The config the server stores for the draft's protocol. */
export function draftConfig(d: ConnectionDraft): Record<string, unknown> {
	switch (d.protocol) {
		case 'mqtt': {
			const topic = d.mqtt.telemetryTopic.trim();
			return {
				topic_prefix: d.mqtt.topicPrefix.trim().replace(/\/+$/, ''),
				qos: d.mqtt.qos,
				...(topic ? { telemetry_topic: topic } : {})
			};
		}
		case 'websocket':
			return { url: d.ws.url.trim(), reconnect_interval_ms: d.ws.reconnectMs };
		case 'serial_tc4':
			return tc4Config(d.tc4);
		case 'phidget22':
			return phidgetConfig(d.phidget);
		case 'modbus_rtu':
			return rtuConfig(d.rtu);
		default:
			return {
				host: d.tcp.host.trim(),
				port: d.tcp.port,
				unit_id: d.tcp.unitId,
				poll_interval_ms: d.tcp.pollIntervalMs,
				...(d.tcp.timeoutMs ? { timeout_ms: d.tcp.timeoutMs } : {})
			};
	}
}

/** What the form still needs, in words; null when it can be saved. `needs` are a plan's
 * (`host`, `port`, `telemetry_topic`: what an Artisan file cannot know). */
export function draftProblem(d: ConnectionDraft, needs: string[] = []): string | null {
	switch (d.protocol) {
		case 'mqtt': {
			const p = d.mqtt.topicPrefix.trim().replace(/\/+$/, '');
			if (p === '' || p === 'roaster')
				return 'Give the topic prefix: roaster/ and the roaster’s id.';
			if (needs.includes('telemetry_topic') && d.mqtt.telemetryTopic.trim() === '')
				return 'Give the MQTT topic its data arrives on.';
			return null;
		}
		case 'websocket': {
			const u = d.ws.url.trim();
			if (!/^wss?:\/\/[^/]+/i.test(u)) return 'Give the WebSocket address: ws://host:port/path.';
			if (needs.includes('host') && /\/\/(127\.0\.0\.1|localhost)([:/]|$)/i.test(u))
				return "Replace 127.0.0.1 with the roaster's address.";
			return null;
		}
		case 'modbus_tcp':
			if (d.tcp.host.trim() === '') return "Give the controller's address.";
			if (!(d.tcp.port > 0 && d.tcp.port <= 65535)) return 'The port is 1 to 65535.';
			return null;
		case 'modbus_rtu':
			return rtuReady(d.rtu) ? null : 'Give the serial port and a unit ID of 1 to 247.';
		case 'serial_tc4':
			return tc4Problems(d.tc4)[0] ?? null;
		case 'phidget22':
			return phidgetProblems(d.phidget)[0] ?? null;
	}
	return null;
}

/** A roaster's id from its name, as MQTT topics and the CLI want it: `Home roaster` → `home_roaster`. */
export function idFromName(name: string): string {
	return name
		.normalize('NFKD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '_')
		.replace(/^_+|_+$/g, '')
		.slice(0, 48);
}
