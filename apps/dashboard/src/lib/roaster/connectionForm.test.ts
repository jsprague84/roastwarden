import { describe, expect, it } from 'vitest';
import {
	connectGuideUrl,
	draftConfig,
	draftProblem,
	followId,
	idFromName,
	mqttPrefixFor,
	newDraft,
	PROTOCOL_CHOICES
} from './connectionForm.js';

describe('one connection form', () => {
	it('starts on MQTT with the prefix the server would use', () => {
		const d = newDraft('home_roaster');
		expect(d.protocol).toBe('mqtt');
		expect(draftConfig(d)).toEqual({ topic_prefix: 'roaster/home_roaster', qos: 0 });
		expect(draftProblem(d)).toBeNull();
	});

	it('refuses the bare roaster/ prefix that heard nothing', () => {
		const d = newDraft('');
		expect(d.mqtt.topicPrefix).toBe('roaster/');
		expect(draftProblem(d)).toMatch(/topic prefix/);
	});

	it("follows the roaster's id until the prefix is edited", () => {
		const d = newDraft('');
		followId(d, '', 'drum_1');
		expect(d.mqtt.topicPrefix).toBe('roaster/drum_1');
		followId(d, 'drum_1', 'drum_2');
		expect(d.mqtt.topicPrefix).toBe('roaster/drum_2');
		d.mqtt.topicPrefix = 'tele/drum';
		followId(d, 'drum_2', 'drum_3');
		expect(d.mqtt.topicPrefix).toBe('tele/drum');
	});

	it('keeps a telemetry topic of its own and trims a trailing slash', () => {
		const d = newDraft('x');
		d.mqtt.topicPrefix = 'cmnd/roaster/';
		d.mqtt.telemetryTopic = ' tele/roaster/SENSOR ';
		expect(draftConfig(d)).toEqual({
			topic_prefix: 'cmnd/roaster',
			qos: 0,
			telemetry_topic: 'tele/roaster/SENSOR'
		});
	});

	it('asks for what a plan cannot know', () => {
		const d = newDraft('x');
		expect(draftProblem(d, ['telemetry_topic'])).toMatch(/topic its data/);
		d.protocol = 'websocket';
		d.ws.url = 'ws://127.0.0.1:8080/ws';
		expect(draftProblem(d)).toBeNull();
		expect(draftProblem(d, ['host'])).toMatch(/127\.0\.0\.1/);
		d.ws.url = 'ws://';
		expect(draftProblem(d)).toMatch(/WebSocket address/);
	});

	it('prefills a roaster found on the network', () => {
		const tcp = newDraft('plc', { protocol: 'modbus_tcp', host: '10.0.0.5', port: '5020' });
		expect(draftConfig(tcp)).toEqual({
			host: '10.0.0.5',
			port: 5020,
			unit_id: 1,
			poll_interval_ms: 1000
		});
		const ws = newDraft('ws', { protocol: 'websocket', host: '10.0.0.6', port: 8080 });
		expect(ws.ws.url).toBe('ws://10.0.0.6:8080/');
		expect(newDraft('x', { protocol: 'nonsense' }).protocol).toBe('mqtt');
		expect(draftProblem(newDraft('x', { protocol: 'modbus_tcp' }))).toMatch(/address/);
		// A Phidget22 network server: its host and port
		const ph = newDraft('phidgetsbc', { protocol: 'phidget22', host: '192.168.1.20', port: 5661 });
		expect(ph.protocol).toBe('phidget22');
		expect(draftConfig(ph)).toMatchObject({
			host: '192.168.1.20',
			port: 5661,
			ingest: { fields: { bt: 'ch0', et: 'ch1' } }
		});
		expect(draftProblem(ph)).toBeNull();
		expect(newDraft('x', { protocol: 'phidget22', host: 'h' }).phidget.port).toBe(5661);
		// Another protocol's find leaves the Phidget's host empty
		expect(newDraft('x', { protocol: 'modbus_tcp', host: 'h' }).phidget.host).toBe('');
	});

	it('writes the serial forms through their own helpers', () => {
		const d = newDraft('x');
		d.protocol = 'modbus_rtu';
		expect(draftConfig(d)).toMatchObject({ port: '/dev/ttyUSB0', baud: 9600, unit_id: 1 });
		d.rtu.unitId = 0;
		expect(draftProblem(d)).toMatch(/unit ID/);
		d.protocol = 'serial_tc4';
		expect(draftConfig(d)).toMatchObject({ port: '/dev/ttyUSB0', bt_channel: 2 });
		d.tc4.port = '';
		expect(draftProblem(d)).toMatch(/serial port/);
	});

	it('writes a Phidget board through its own helper, its host asked for', () => {
		const d = newDraft('x');
		d.protocol = 'phidget22';
		expect(draftProblem(d)).toMatch(/computer the board is plugged into/);
		d.phidget.host = 'phidgetsbc.local';
		d.phidget.et = '';
		expect(draftProblem(d)).toBeNull();
		expect(draftConfig(d)).toMatchObject({
			host: 'phidgetsbc.local',
			port: 5661,
			thermocouple_type: 'K',
			ingest: { fields: { bt: 'ch0' } }
		});
	});

	it('offers every protocol once, MQTT first', () => {
		const ps = PROTOCOL_CHOICES.map((c) => c.protocol);
		expect(ps[0]).toBe('mqtt');
		expect(new Set(ps).size).toBe(6);
		expect(ps).toContain('phidget22');
		expect(mqttPrefixFor(' a ')).toBe('roaster/a');
	});

	it('links the guide at the running revision, or not at all', () => {
		expect(connectGuideUrl('https://git.example.org/me/rr/src/commit/abc1234/')).toBe(
			'https://git.example.org/me/rr/src/commit/abc1234/docs/connect-your-roaster.md'
		);
		expect(connectGuideUrl(null)).toBeNull();
		expect(connectGuideUrl('  ')).toBeNull();
	});

	it("makes the roaster's id from its name", () => {
		expect(idFromName('Home roaster')).toBe('home_roaster');
		expect(idFromName('  Café Drum #2 ')).toBe('cafe_drum_2');
		expect(idFromName('***')).toBe('');
	});
});
