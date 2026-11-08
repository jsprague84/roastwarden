import { describe, expect, it } from 'vitest';
import {
	bitKind,
	connectionSummary,
	framing,
	isModbus,
	protocolLabel,
	readOnlyKind,
	RTU_DEFAULTS,
	rtuConfig,
	rtuReady
} from './connection.js';

describe('connections in words', () => {
	it('names every protocol, Modbus RTU included', () => {
		expect(protocolLabel('modbus_rtu')).toBe('Modbus RTU');
		expect(protocolLabel('serial_tc4')).toBe('Serial (TC4)');
		expect(protocolLabel('phidget22')).toBe('Phidgets (Phidget22 network server)');
		expect(isModbus('phidget22')).toBe(false);
		expect(isModbus('modbus_rtu')).toBe(true);
		expect(isModbus('modbus_tcp')).toBe(true);
		expect(isModbus('serial_tc4')).toBe(false);
	});

	it('summarises an RTU connection with its framing and unit', () => {
		expect(framing(9600, 'none', 1)).toBe('9600 8N1');
		expect(framing(19200, 'even', 1)).toBe('19200 8E1');
		expect(
			connectionSummary('modbus_rtu', {
				port: '/dev/ttyUSB0',
				baud: 19200,
				parity: 'odd',
				stop_bits: 2,
				unit_id: 3
			})
		).toBe('/dev/ttyUSB0 19200 8O2 unit 3');
		expect(connectionSummary('modbus_rtu', { port: '/dev/ttyUSB1' })).toBe(
			'/dev/ttyUSB1 9600 8N1 unit 1'
		);
		expect(
			connectionSummary('modbus_rtu', {
				port: '/dev/ttyUSB0',
				framing: 'ascii',
				data_bits: 7,
				parity: 'even',
				baud: 9600,
				unit_id: 1
			})
		).toBe('/dev/ttyUSB0 ASCII 9600 7E1 unit 1');
		// RTU is always 8 data bits, whatever the config says
		expect(connectionSummary('modbus_rtu', { port: '/dev/ttyUSB0', data_bits: 7 })).toBe(
			'/dev/ttyUSB0 9600 8N1 unit 1'
		);
		expect(framing(9600, 'even', 1, 7)).toBe('9600 7E1');
		expect(connectionSummary('modbus_tcp', { host: '10.0.0.5' })).toBe('10.0.0.5:502');
		expect(connectionSummary('serial_tc4', { port: '/dev/ttyACM0' })).toContain('/dev/ttyACM0');
		expect(connectionSummary('phidget22', { host: 'phidgetsbc.local', port: 5661 })).toBe(
			'phidgetsbc.local:5661 · BT ch0 · ET ch1 · type K'
		);
	});

	it('builds the config the server reads, 8 data bits unless ASCII', () => {
		expect(rtuConfig({ ...RTU_DEFAULTS, port: ' /dev/ttyUSB0 ', parity: 'even' })).toEqual({
			port: '/dev/ttyUSB0',
			baud: 9600,
			parity: 'even',
			data_bits: 8,
			stop_bits: 1,
			unit_id: 1,
			poll_interval_ms: 1000
		});
		// The data bits count only for ASCII, which says so
		expect(rtuConfig({ ...RTU_DEFAULTS, dataBits: 7 }).data_bits).toBe(8);
		expect(rtuConfig({ ...RTU_DEFAULTS, framing: 'ascii', dataBits: 7, parity: 'even' })).toEqual({
			port: '/dev/ttyUSB0',
			framing: 'ascii',
			baud: 9600,
			parity: 'even',
			data_bits: 7,
			stop_bits: 1,
			unit_id: 1,
			poll_interval_ms: 1000
		});
		expect(rtuReady(RTU_DEFAULTS)).toBe(true);
		expect(rtuReady({ ...RTU_DEFAULTS, port: '' })).toBe(false);
		expect(rtuReady({ ...RTU_DEFAULTS, unitId: 0 })).toBe(false);
		expect(rtuReady({ ...RTU_DEFAULTS, unitId: 248 })).toBe(false);
	});

	it('knows which register kinds are read-only or one bit', () => {
		expect(readOnlyKind('input')).toBe(true);
		expect(readOnlyKind('discrete')).toBe(true);
		expect(readOnlyKind('holding')).toBe(false);
		expect(bitKind('coil')).toBe(true);
		expect(bitKind('holding')).toBe(false);
	});
});
