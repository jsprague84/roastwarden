import { describe, expect, it } from 'vitest';
import {
	connectionConfigFromPlan,
	declaredSummary,
	filterMachines,
	needLabel,
	placeholderHost,
	planExtras,
	prefillFromPlan,
	suggestedId
} from './artisan.js';
import { tc4Config } from './tc4.js';
import { phidgetConfig } from './phidget.js';
import { rtuConfig } from './connection.js';
import type { ArtisanMachine, ArtisanPlan } from '#lib/api/devices.js';

const m = (key: string, supported = true, device = 'Modbus'): ArtisanMachine => {
	const [brand, model] = key.split('/');
	return {
		key,
		brand,
		model: model.replace(/_/g, ' '),
		name: `${brand} ${model.replace(/_/g, ' ')}`,
		artisan_device: supported ? 29 : 79,
		artisan_device_name: device,
		supported,
		...(supported ? {} : { why: `Artisan reads it through ${device}` })
	};
};
const LIST = [
	m('Berto/Autonics_Control'),
	m('Coffed/SR5_manual'),
	m('Giesen/W6A', false, 'Siemens S7')
];

const rtuPlan: ArtisanPlan = {
	machine: LIST[0],
	source: "Artisan's machine catalogue, commit abc",
	protocol: 'modbus_rtu',
	config: {
		baud: 9600,
		parity: 'even',
		data_bits: 8,
		stop_bits: 2,
		unit_id: 1,
		poll_interval_ms: 2000,
		timeout_ms: 400
	},
	registers: [
		{
			register_type: 'input',
			address: 1000,
			name: 'BT',
			data_type: 'uint16',
			scale_factor: 0.1,
			unit: 'C',
			writable: false,
			role: 'bt',
			unit_id: null
		},
		{
			register_type: 'input',
			address: 1000,
			name: 'ET',
			data_type: 'uint16',
			scale_factor: 0.1,
			unit: 'C',
			writable: false,
			role: 'et',
			unit_id: 2
		},
		{
			register_type: 'holding',
			address: 1,
			name: 'Burner out',
			data_type: 'uint16',
			writable: true,
			role: 'heater',
			unit_id: 3
		}
	],
	setup: {
		kind: 'drum_gas',
		control: 'off',
		outputs: {
			heater: { label: 'Burner', min: 0, max: 100, step: 0 },
			fan: { label: 'Air', min: 0, max: 100, step: 0 },
			drum: null,
			damper: null
		},
		fan_floor_pct: 0,
		channels: [{ key: 'it', label: 'IT', unit: 'C', max: null }]
	},
	needs: ['port'],
	notes: ['Modbus RTU at 9600 baud: give the serial port']
};

describe('Artisan machine catalogue', () => {
	it('filters by every word, the supported ones unless all', () => {
		expect(filterMachines(LIST, '', false).map((x) => x.key)).toEqual([
			'Berto/Autonics_Control',
			'Coffed/SR5_manual'
		]);
		expect(filterMachines(LIST, 'berto autonics', false).map((x) => x.key)).toEqual([
			'Berto/Autonics_Control'
		]);
		expect(filterMachines(LIST, 'sr5 manual', false)).toHaveLength(1);
		expect(filterMachines(LIST, 'giesen', false)).toEqual([]);
		expect(filterMachines(LIST, 's7', true).map((x) => x.key)).toEqual(['Giesen/W6A']);
	});

	it('suggests a roaster id from the key', () => {
		expect(suggestedId(LIST[0])).toBe('berto_autonics_control');
		expect(suggestedId(m('Dutch Master Roaster/DMR15-A'))).toBe('dutch_master_roaster_dmr15_a');
	});
});

describe('a plan as the wizard fields', () => {
	it('fills a serial machine, its port left to you', () => {
		const p = prefillFromPlan(rtuPlan)!;
		expect(p.protocol).toBe('modbus_rtu');
		expect(p.tcp).toBeNull();
		expect(p.rtu).toMatchObject({
			port: '',
			baud: 9600,
			parity: 'even',
			stopBits: 2,
			unitId: 1,
			pollIntervalMs: 2000,
			timeoutMs: 400
		});
		expect(p.registers.map((r) => r.unit_id)).toEqual([null, 2, 3]);
		expect(p.registers[2]).toMatchObject({
			role: 'heater',
			writable: true,
			scale_factor: 1,
			byte_order: 'AB'
		});
		expect(p.kind).toBe('drum_gas');
		expect(p.needs).toEqual(['port']);
		expect(declaredSummary(p)).toBe('outputs Burner, Air; channels IT');
	});

	it('fills a TCP machine, keeping a host the file names', () => {
		const tcp: ArtisanPlan = {
			...rtuPlan,
			protocol: 'modbus_tcp',
			config: {
				host: '192.168.1.27',
				port: 502,
				unit_id: 1,
				poll_interval_ms: 1000,
				timeout_ms: 1000
			},
			needs: []
		};
		const p = prefillFromPlan(tcp)!;
		expect(p.rtu).toBeNull();
		expect(p.tcp).toEqual({
			host: '192.168.1.27',
			port: 502,
			unitId: 1,
			pollIntervalMs: 1000,
			timeoutMs: 1000
		});
		expect(prefillFromPlan({ ...tcp, needs: ['host'] })!.tcp!.host).toBe('');
	});

	it('gives nothing for a machine Roastwarden cannot set up', () => {
		expect(
			prefillFromPlan({ machine: LIST[2], source: 'x', registers: [], needs: [], notes: [] })
		).toBeNull();
	});

	it('says when nothing is declared', () => {
		expect(
			declaredSummary({
				outputs: { heater: null, fan: null, drum: null, damper: null },
				channels: []
			})
		).toBe('no outputs (records only)');
	});

	it('keeps Modbus ASCII framing and 7 data bits', () => {
		const p = prefillFromPlan({
			...rtuPlan,
			config: {
				framing: 'ascii',
				data_bits: 7,
				parity: 'even',
				baud: 9600,
				stop_bits: 1,
				unit_id: 1,
				poll_interval_ms: 1000
			}
		})!;
		expect(p.rtu).toMatchObject({ framing: 'ascii', dataBits: 7, parity: 'even' });
		expect(rtuConfig({ ...p.rtu!, port: '/dev/ttyUSB0' })).toMatchObject({
			framing: 'ascii',
			data_bits: 7
		});
		// RTU stays 8 data bits
		expect(prefillFromPlan(rtuPlan)!.rtu).toMatchObject({ framing: 'rtu', dataBits: 8 });
	});
});

const setup = rtuPlan.setup!;

const tc4Plan: ArtisanPlan = {
	machine: m('Kaleido/Legacy'),
	source: 'x',
	protocol: 'serial_tc4',
	config: {
		baud: 57600,
		poll_interval_ms: 1500,
		reply_format: 'active',
		et_channel: 1,
		bt_channel: 2,
		t3_channel: 3,
		fan_output: 'ot2',
		commands: {
			heater: { payload: 'OT1,{value}', decimals: 0 },
			drum: { payload: 'IO3,{value}', decimals: 0 }
		}
	},
	registers: [],
	setup,
	needs: ['port'],
	notes: []
};

const wsPlan: ArtisanPlan = {
	machine: m('Probat/Sample_Roaster'),
	source: 'x',
	protocol: 'websocket',
	config: {
		url: 'ws://127.0.0.1:80/WebSocket',
		reconnect_interval_ms: 2000,
		ingest: { units: 'C', fields: { bt: { path: 'data.beanTemp' }, et: { path: 'data.airTemp' } } },
		poll: {
			message: '{"command":"getCurrentRoastingStep","id":{id},"roasterID":0}',
			interval_ms: 1000
		},
		commands: { heater: { payload: '{"command":"setBurner","value":{value}}' } }
	},
	registers: [],
	setup,
	needs: ['host'],
	notes: []
};

const mqttPlan: ArtisanPlan = {
	machine: m('ROEST/L200_Ultra'),
	source: 'x',
	protocol: 'mqtt',
	config: { qos: 0, ingest: { units: 'F', fields: { bt: { path: 'bt' } } } },
	registers: [],
	setup,
	needs: ['telemetry_topic'],
	notes: ["Bridge ROEST's cloud broker"]
};

const phidgetPlan: ArtisanPlan = {
	machine: m('Phidgets/1048'),
	source: 'x',
	protocol: 'phidget22',
	config: {
		host: '',
		port: 5661,
		poll_interval_ms: 1000,
		data_interval_ms: 256,
		thermocouple_type: 'J',
		ingest: { fields: { bt: 'ch0', et: 'ch1', exhaust: 'ch2' } }
	},
	registers: [],
	setup,
	needs: ['host'],
	notes: []
};

describe('plans that are not Modbus', () => {
	it('fills a Phidgets machine, the host left to you, its mapping kept', () => {
		const p = prefillFromPlan(phidgetPlan)!;
		expect(p.protocol).toBe('phidget22');
		expect(p.tc4).toBeNull();
		expect(p.phidget).toMatchObject({
			host: '',
			port: 5661,
			thermocoupleType: 'J',
			bt: 'ch0',
			et: 'ch1'
		});
		expect(p.needs.map(needLabel)).toEqual(["roaster's address"]);
		const cfg = connectionConfigFromPlan(
			p,
			'phidget22',
			phidgetConfig({ ...p.phidget!, host: 'phidgetsbc.local' })
		);
		expect(cfg).toMatchObject({
			host: 'phidgetsbc.local',
			thermocouple_type: 'J',
			ingest: { fields: { bt: 'ch0', et: 'ch1', exhaust: 'ch2' } }
		});
		// The form carries the whole config: a hub port cleared in it stays cleared
		const hub = prefillFromPlan({
			...phidgetPlan,
			config: { ...phidgetPlan.config, hub_port: 2 }
		})!;
		expect(
			connectionConfigFromPlan(hub, 'phidget22', phidgetConfig({ ...hub.phidget!, hubPort: null }))
		).not.toHaveProperty('hub_port');
	});

	it('fills a TC4 machine so its settings round-trip, the port left to you', () => {
		const p = prefillFromPlan(tc4Plan)!;
		expect(p.protocol).toBe('serial_tc4');
		expect(p.tcp).toBeNull();
		expect(p.rtu).toBeNull();
		expect(p.tc4).toMatchObject({
			port: '',
			baud: 57600,
			poll_interval_ms: 1500,
			reply_format: 'active',
			t3_channel: 3,
			t4_channel: 0,
			fan_output: 'ot2'
		});
		expect(p.registers).toEqual([]);
		const cfg = connectionConfigFromPlan(
			p,
			'serial_tc4',
			tc4Config({ ...p.tc4!, port: '/dev/ttyUSB0' })
		);
		expect(cfg).toMatchObject({
			port: '/dev/ttyUSB0',
			baud: 57600,
			t3_channel: 3,
			fan_output: 'ot2',
			reply_format: 'active'
		});
		expect(cfg.commands).toEqual(tc4Plan.config!.commands);
		expect(planExtras(p.extra)).toBe('2 command templates');
	});

	it('fills a WebSocket machine, its placeholder host flagged', () => {
		const p = prefillFromPlan(wsPlan)!;
		expect(p.ws).toEqual({ url: 'ws://127.0.0.1:80/WebSocket', reconnectMs: 2000 });
		expect(p.needs).toEqual(['host']);
		expect(placeholderHost(p.ws!.url)).toBe(true);
		expect(placeholderHost('ws://192.168.1.40:80/WebSocket')).toBe(false);
		expect(placeholderHost('ws://localhost/')).toBe(true);
		const cfg = connectionConfigFromPlan(p, 'websocket', {
			url: 'ws://192.168.1.40:80/WebSocket',
			reconnect_interval_ms: 2000
		});
		expect(cfg).toEqual({ ...wsPlan.config, url: 'ws://192.168.1.40:80/WebSocket' });
		expect(planExtras(p.extra)).toBe(
			'2 mapped fields, the getCurrentRoastingStep poll, 1 command template'
		);
	});

	it('fills an MQTT machine, the telemetry topic left to you', () => {
		const p = prefillFromPlan(mqttPlan)!;
		expect(p.mqtt).toEqual({ telemetryTopic: '', qos: 0 });
		expect(p.needs.map(needLabel)).toEqual(['MQTT topic its data arrives on']);
		const cfg = connectionConfigFromPlan(p, 'mqtt', {
			topic_prefix: 'roaster/roest',
			telemetry_topic: 'roest/bridge/data',
			qos: 0
		});
		expect(cfg).toEqual({
			qos: 0,
			ingest: mqttPlan.config!.ingest,
			topic_prefix: 'roaster/roest',
			telemetry_topic: 'roest/bridge/data'
		});
		expect(planExtras(p.extra)).toBe('1 mapped field in °F');
	});

	it("sends the form alone off the plan's protocol, and for Modbus", () => {
		const p = prefillFromPlan(wsPlan)!;
		expect(connectionConfigFromPlan(p, 'mqtt', { topic_prefix: 'roaster/x' })).toEqual({
			topic_prefix: 'roaster/x'
		});
		expect(connectionConfigFromPlan(null, 'websocket', { url: 'ws://a/' })).toEqual({
			url: 'ws://a/'
		});
		// ASCII switched back to RTU in the form stays RTU
		const ascii = prefillFromPlan({ ...rtuPlan, config: { framing: 'ascii', data_bits: 7 } })!;
		expect(
			connectionConfigFromPlan(ascii, 'modbus_rtu', { port: '/dev/ttyUSB0', data_bits: 8 })
		).toEqual({ port: '/dev/ttyUSB0', data_bits: 8 });
	});

	it('names every need, and nothing extra for a plain config', () => {
		expect(['host', 'port', 'telemetry_topic'].map(needLabel)).toEqual([
			"roaster's address",
			'serial port',
			'MQTT topic its data arrives on'
		]);
		expect(planExtras({ baud: 9600 })).toBe('');
		expect(planExtras({ poll: { interval_ms: 1000 } })).toBe('a poll');
	});
});
