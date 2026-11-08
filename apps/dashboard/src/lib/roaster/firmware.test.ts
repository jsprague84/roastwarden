import { describe, expect, it } from 'vitest';
import {
	addressProblem,
	defaultAddress,
	firmwareRequest,
	formProblem,
	idProblem,
	missingNote,
	wifiProblem,
	type FirmwareForm
} from './firmware.js';

const form: FirmwareForm = {
	kind: 'fluid_bed',
	roasterId: ' home_roaster ',
	password: '',
	address: '192.168.1.50',
	port: null,
	wifiName: 'HomeNet',
	wifiPassword: 'secret-pw',
	board: 'max31855',
	etProbe: true
};

describe('the firmware form', () => {
	it('starts from the address the dashboard was opened at, never localhost', () => {
		expect(defaultAddress('192.168.1.50')).toBe('192.168.1.50');
		expect(defaultAddress('roaster-pi.local')).toBe('roaster-pi.local');
		expect(defaultAddress('localhost')).toBe('');
		expect(defaultAddress('127.0.0.1')).toBe('');
	});

	it('checks what the server checks', () => {
		expect(idProblem('my_roaster')).toBeNull();
		expect(idProblem('a/b')).toMatch(/Letters/);
		expect(idProblem('')).toMatch(/id/);
		expect(addressProblem('http://192.168.1.50')).toMatch(/without http/);
		expect(addressProblem('192.168.1.50:1883')).toMatch(/port/);
		expect(addressProblem('localhost')).toMatch(/ESP32 itself/);
		expect(wifiProblem('Net', 'short')).toMatch(/8 characters/);
		expect(wifiProblem('Net', '')).toBeNull(); // an open network
		expect(wifiProblem('x'.repeat(33), 'longenough')).toMatch(/32 bytes/);
		expect(formProblem(form)).toBeNull();
	});

	it('sends only what was given', () => {
		expect(firmwareRequest(form)).toEqual({
			kind: 'fluid_bed',
			roaster_id: 'home_roaster',
			server_address: '192.168.1.50',
			board: 'max31855',
			et_probe: true,
			wifi_name: 'HomeNet',
			wifi_password: 'secret-pw'
		});
		const bare = firmwareRequest({ ...form, wifiName: ' ', password: 'pw', port: 1884 });
		expect(bare.wifi_name).toBeUndefined();
		expect(bare.wifi_password).toBeUndefined();
		expect(bare.password).toBe('pw');
		expect(bare.port).toBe(1884);
	});

	it('says what is left to fill in by hand', () => {
		expect(missingNote([], 'r')).toMatch(/Every connection setting/);
		expect(missingNote(['password', 'wifi'], 'my_roaster')).toBe(
			"Still to fill in at the top of the sketch: WIFI_NAME and WIFI_PASSWORD, ROASTER_PASSWORD (my_roaster's MQTT password)."
		);
	});
});
