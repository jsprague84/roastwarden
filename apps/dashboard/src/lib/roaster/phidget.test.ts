import { describe, expect, it } from 'vitest';
import {
	boardLine,
	defaultPhidgetSettings,
	PASSWORD_MASK,
	phidgetConfig,
	phidgetFields,
	phidgetProblems,
	phidgetSettingsOf,
	phidgetSummary
} from './phidget.js';

describe('a Phidgets board through the Phidget22 network server', () => {
	it('starts on port 5661, BT ch0, ET ch1, type K, and needs only the host', () => {
		const s = defaultPhidgetSettings();
		expect(phidgetProblems(s)).toEqual([
			'Give the computer the board is plugged into (its Phidget22 network server).'
		]);
		expect(phidgetProblems({ ...s, host: 'phidgetsbc.local' })).toEqual([]);
		expect(phidgetConfig({ ...s, host: ' phidgetsbc.local ' })).toEqual({
			host: 'phidgetsbc.local',
			port: 5661,
			poll_interval_ms: 1000,
			data_interval_ms: 256,
			thermocouple_type: 'K',
			rtd_type: 'pt100_3850',
			rtd_wires: 2,
			ingest: { fields: { bt: 'ch0', et: 'ch1' } }
		});
	});

	it('round-trips a whole config, its extra channels, scales and per-input types kept', () => {
		const cfg = {
			host: '192.168.1.20',
			port: 5662,
			serial_number: 370001,
			hub_port: 2,
			poll_interval_ms: 500,
			data_interval_ms: 128,
			thermocouple_type: 'J',
			thermocouple_types: { ch2: 'T' },
			rtd_type: 'pt1000_3920',
			rtd_wires: 3,
			ingest: { fields: { bt: { path: 'ch1', offset: -1.5 }, et: 'ch0', exhaust: 'ch2' } }
		};
		const s = phidgetSettingsOf(cfg);
		expect(s).toMatchObject({
			bt: 'ch1',
			et: 'ch0',
			serialNumber: 370001,
			hubPort: 2,
			thermocoupleType: 'J',
			rtdWires: 3
		});
		expect(phidgetConfig(s)).toEqual(cfg);
		// A new BT input keeps its offset
		expect(phidgetFields({ ...s, bt: 'ch3' }).bt).toEqual({ path: 'ch3', offset: -1.5 });
		// ET none drops it, the extra stays
		expect(phidgetFields({ ...s, et: '' })).toEqual({
			bt: { path: 'ch1', offset: -1.5 },
			exhaust: 'ch2'
		});
	});

	it('reads no mapping as BT ch0, ET ch1, as the server does', () => {
		const s = phidgetSettingsOf({ host: 'h' });
		expect(s).toMatchObject({
			bt: 'ch0',
			et: 'ch1',
			port: 5661,
			thermocoupleType: 'K',
			serialNumber: null,
			hubPort: null
		});
		expect(phidgetSummary({ host: 'h' })).toBe('h:5661 · BT ch0 · ET ch1 · type K');
	});

	it('keeps a saved password without knowing it, and can forget it', () => {
		const s = phidgetSettingsOf({ host: 'h', password: PASSWORD_MASK });
		expect(s.passwordSaved).toBe(true);
		expect(s.password).toBe('');
		expect(phidgetConfig(s).password).toBe(PASSWORD_MASK);
		expect(phidgetConfig({ ...s, password: 'new' }).password).toBe('new');
		expect(phidgetConfig({ ...s, passwordSaved: false })).not.toHaveProperty('password');
	});

	it('says what is wrong', () => {
		const s = { ...defaultPhidgetSettings('h') };
		expect(phidgetProblems({ ...s, et: 'ch0' })).toContain('BT and ET are on the same input.');
		expect(phidgetProblems({ ...s, bt: '', et: '' })).toContain('Pick the input BT (or ET) is on.');
		expect(phidgetProblems({ ...s, hubPort: 6 })).toContain('The hub port is 0 to 5.');
		expect(phidgetProblems({ ...s, dataIntervalMs: 4 })).toContain(
			'The data interval is 8 to 60000 ms.'
		);
		expect(phidgetProblems({ ...s, port: 0 })[0]).toMatch(/port is 1 to 65535/);
		expect(phidgetProblems({ ...s, host: 'two words' })[0]).toMatch(/computer the board/);
		expect(phidgetProblems({ ...s, rtdWires: 5 })).toContain('An RTD has 2, 3 or 4 wires.');
	});

	it('names the board it picks, or none for the first', () => {
		expect(boardLine({ serialNumber: null, hubPort: null })).toBe('');
		expect(boardLine({ serialNumber: 370001, hubPort: 2 })).toBe('board 370001 hub port 2');
		expect(
			phidgetSummary({
				host: 'h',
				serial_number: 5,
				ingest: { fields: { bt: 'ch2', exhaust: 'ch3' } }
			})
		).toBe('h:5661 · BT ch2 · +1 · type K · board 5');
	});
});
