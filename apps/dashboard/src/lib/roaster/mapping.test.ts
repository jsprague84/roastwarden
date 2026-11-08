import { describe, it, expect } from 'vitest';
import {
	mappingFromConfig,
	ingestOf,
	commandsOf,
	configWithMapping,
	mappingProblems,
	emptyField,
	fieldOf,
	hasMapping,
	prettyPayload
} from './mapping.js';

const tasmota = {
	topic_prefix: 'cmnd/roaster',
	telemetry_topic: 'tele/roaster/SENSOR',
	qos: 0,
	ingest: {
		units: 'F',
		fields: {
			bt: 'MAX31855.Temperature',
			fan: { path: 'Fan', scale: 0.5 },
			exhaust: { path: 'probes[2].value', unit: 'F' },
			heater: { topic: 'stat/roaster/POWER' }
		}
	},
	commands: { heater: { topic: 'cmnd/roaster/Dimmer', payload: '{value}', decimals: 0 } }
};

describe('mapping', () => {
	it('reads a saved config into rows and writes the same config back', () => {
		const form = mappingFromConfig('mqtt', tasmota);
		expect(form.units).toBe('F');
		expect(form.topicPrefix).toBe('cmnd/roaster');
		expect(form.telemetryTopic).toBe('tele/roaster/SENSOR');
		expect(form.fields.map((f) => f.key)).toEqual(['bt', 'fan', 'exhaust', 'heater']);
		expect(form.fields[1].scale).toBe('0.5');
		expect(form.fields[2].unit).toBe('F');
		expect(form.fields[3].topic).toBe('stat/roaster/POWER');
		expect(form.templates[0]).toMatchObject({
			key: 'heater',
			topic: 'cmnd/roaster/Dimmer',
			decimals: '0'
		});
		expect(configWithMapping('mqtt', tasmota, form)).toEqual(tasmota);
		expect(mappingProblems('mqtt', form)).toEqual([]);
	});

	it('leaves out what is not mapped: no ingest, no commands, default topics', () => {
		const cfg = { topic_prefix: 'roaster/x', qos: 0, ingest: { fields: { bt: 'a' } } };
		const form = mappingFromConfig('mqtt', cfg);
		form.fields = [];
		form.topicPrefix = '';
		expect(configWithMapping('mqtt', cfg, form)).toEqual({ qos: 0 });
		expect(ingestOf('websocket', { ...form, fields: [emptyField()] })).toBeNull();
		expect(commandsOf('mqtt', form)).toBeNull();
	});

	it('writes a plain path as a string and drops a unit and topic where they do not apply', () => {
		const form = mappingFromConfig('websocket', {});
		form.fields = [
			{ ...emptyField('bt'), path: 'data.BT', unit: 'F', topic: 'x' },
			{ ...emptyField('et'), path: 'data.ET', offset: '-2' }
		];
		expect(ingestOf('websocket', form)).toEqual({
			fields: { bt: 'data.BT', et: { path: 'data.ET', offset: -2 } }
		});
	});

	it('maps a TC4 by columns and units', () => {
		const form = mappingFromConfig('serial_tc4', {
			port: '/dev/ttyUSB0',
			ingest: { units: 'F', columns: ['ambient', 'et', 'bt', '', 'fan'] }
		});
		expect(form.columns).toEqual(['ambient', 'et', 'bt', '', 'fan']);
		expect(ingestOf('serial_tc4', form)).toEqual({
			units: 'F',
			columns: ['ambient', 'et', 'bt', '', 'fan']
		});
		expect(ingestOf('serial_tc4', { ...form, columns: [] })).toEqual({ units: 'F' });
		expect(ingestOf('serial_tc4', { ...form, columns: [''], units: 'C' })).toBeNull();
		expect(mappingProblems('serial_tc4', { ...form, columns: ['ambient', 'fan'] })).toContain(
			'the columns must name bt or et (or both)'
		);
	});

	it('takes an extra TC4 column’s own unit', () => {
		const form = mappingFromConfig('serial_tc4', {
			port: 'x',
			ingest: { columns: ['et', 'bt', 'exhaust:F', 'gas:none'] }
		});
		expect(form.columns).toEqual(['et', 'bt', 'exhaust:F', 'gas:none']);
		expect(mappingProblems('serial_tc4', form)).toEqual([]);
		expect(mappingProblems('serial_tc4', { ...form, columns: ['bt', 'exhaust:K'] })).toEqual([
			"'exhaust:K': the unit after ':' is C, F or none"
		]);
		expect(mappingProblems('serial_tc4', { ...form, columns: ['bt:F', 'et'] })).toEqual([
			"'bt:F': a unit is for an extra column; bt follows the device's unit"
		]);
		expect(mappingProblems('serial_tc4', { ...form, columns: ['bt', 'x:F', 'x'] })).toContain(
			"'x' is in two columns"
		);
	});

	it('says what is plainly wrong, in the server’s words', () => {
		const form = mappingFromConfig('mqtt', {});
		form.fields = [
			{ ...emptyField('fan'), path: 'Fan', scale: '0' },
			{ ...emptyField('beanTemp'), path: 'x' },
			{ ...emptyField('t3'), path: 'y', topic: 'a/+/b' }
		];
		form.templates = [{ key: 'pid', topic: '', payload: '', scale: '', decimals: '5' }];
		const p = mappingProblems('mqtt', form);
		expect(p).toContain('map bt or et (or both)');
		expect(p.some((x) => x.startsWith('fan: scale'))).toBe(true);
		expect(p.some((x) => x.startsWith("'beanTemp' is not a role"))).toBe(true);
		expect(p.some((x) => x.includes('wildcard'))).toBe(true);
		expect(p.some((x) => x.startsWith("no template for 'pid'"))).toBe(true);
		expect(p).toContain('pid: decimals 0 to 3');
	});

	it("maps a Phidget's inputs: ch0–ch7, °C, no commands", () => {
		// No mapping: the editor starts from the server's default
		const d = mappingFromConfig('phidget22', { host: 'h' });
		expect(d.fields.map((f) => [f.key, f.path])).toEqual([
			['bt', 'ch0'],
			['et', 'ch1']
		]);
		const form = mappingFromConfig('phidget22', {
			host: 'h',
			ingest: { fields: { bt: 'ch0', exhaust: { path: 'ch2', offset: 1 } } }
		});
		expect(ingestOf('phidget22', form)).toEqual({
			fields: { bt: 'ch0', exhaust: { path: 'ch2', offset: 1 } }
		});
		// A unit or °F never reaches a Phidget's config
		const f2 = {
			...form,
			units: 'F' as const,
			fields: form.fields.map((f) => ({ ...f, unit: 'F' as const }))
		};
		expect(ingestOf('phidget22', f2)).toEqual({
			fields: { bt: 'ch0', exhaust: { path: 'ch2', offset: 1 } }
		});
		expect(mappingProblems('phidget22', form)).toEqual([]);
		expect(
			mappingProblems('phidget22', { ...form, fields: [{ ...emptyField('bt'), path: 'data.BT' }] })
		).toEqual(["bt: 'data.BT' is not a board input: ch0 to ch7"]);
		const withCommand = {
			...form,
			templates: [{ key: 'heater', topic: '', payload: '', scale: '', decimals: '' }]
		};
		expect(mappingProblems('phidget22', withCommand)).toContain(
			'a Phidget connection reads temperatures only: it takes no commands'
		);
		expect(commandsOf('phidget22', withCommand)).toBeNull();
		// The rest of the config is kept, a saved password's mask with it
		const cfg = configWithMapping(
			'phidget22',
			{ host: 'h', password: '********', ingest: { fields: {} } },
			form
		);
		expect(cfg).toEqual({
			host: 'h',
			password: '********',
			ingest: { fields: { bt: 'ch0', exhaust: { path: 'ch2', offset: 1 } } }
		});
	});

	it('names fields and protocols', () => {
		expect(fieldOf('bt')).toBe('beanTemp');
		expect(fieldOf('exhaust')).toBe('exhaust');
		expect(hasMapping('serial_tc4')).toBe(true);
		expect(hasMapping('phidget22')).toBe(true);
		expect(hasMapping('modbus_tcp')).toBe(false);
		expect(prettyPayload('{"a":1}')).toBe('{\n  "a": 1\n}');
		expect(prettyPayload('25.0,180.2,150.1')).toBe('25.0,180.2,150.1');
	});
});
