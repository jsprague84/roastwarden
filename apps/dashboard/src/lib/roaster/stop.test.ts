import { describe, expect, it } from 'vitest';
import { coolEnd, stopButton, STOP_HOT_C, STOP_STALE_MS, type StopInputs } from './stop.js';
import type { Telemetry } from '#lib/api/telemetry.js';

const idle: Telemetry = {
	beanTemp: 27.5,
	envTemp: 27.5,
	heater: 0,
	fan: 0,
	setpoint: 210,
	controlMode: 0,
	heaterEnable: 0
};
const at = (reading: Partial<Telemetry>, more: Partial<StopInputs> = {}): StopInputs => ({
	reading: { ...idle, ...reading },
	heardAt: 1_000_000,
	now: 1_001_000,
	roastOpen: false,
	controllable: true,
	coolEndC: 45,
	...more
});
const kind = (i: StopInputs) => stopButton(i)?.kind ?? null;

describe('stopButton', () => {
	it('shows nothing on a cold idle roaster', () => {
		expect(stopButton(at({}))).toBeNull();
	});

	it('is STOP while something runs or it is hot', () => {
		expect(stopButton(at({ heaterEnable: 1 }))?.why).toBe('the heater is on');
		expect(kind(at({ heater: 12, fan: 60 }))).toBe('stop');
		expect(kind(at({ heaterApplied: 5 }))).toBe('stop');
		expect(stopButton(at({ drum: 55 }))?.why).toBe('the drum is turning');
		expect(stopButton(at({ beanTemp: 45 }))?.why).toBe('the roaster is hot');
		expect(kind(at({ envTemp: 60 }))).toBe('stop');
	});

	it('after an emergency stop: STOP while it cools, then Fan off', () => {
		// heater off, fan at 100 % (the firmware's safe state), still hot
		expect(stopButton(at({ fan: 100, beanTemp: 120, envTemp: 90 }))?.why).toBe(
			'cooling: Fan off once it is below 45 °C'
		);
		expect(kind(at({ fan: 100, beanTemp: 44, envTemp: 46 }))).toBe('stop');
		expect(kind(at({ fan: 100, beanTemp: 44, envTemp: 44 }))).toBe('fan_off');
		expect(kind(at({ fan: 30 }))).toBe('fan_off');
	});

	it('leaves the fan to the server while it cools after DROP', () => {
		expect(kind(at({ fan: 100, beanTemp: 30, envTemp: 30 }, { cooling: true }))).toBe('stop');
	});

	it('uses 50 °C when the sequence has no cool-down temperature', () => {
		expect(coolEnd(0)).toBe(STOP_HOT_C);
		expect(coolEnd(null)).toBe(STOP_HOT_C);
		expect(kind(at({ fan: 100, beanTemp: 51, envTemp: 40 }, { coolEndC: 0 }))).toBe('stop');
		expect(kind(at({ fan: 100, beanTemp: 48, envTemp: 48 }, { coolEndC: null }))).toBe('fan_off');
	});

	it('is STOP for an open roast even without a reading', () => {
		expect(kind(at({}, { roastOpen: true }))).toBe('stop');
		expect(kind(at({}, { roastOpen: true, reading: null, heardAt: null }))).toBe('stop');
	});

	it('shows nothing when the roaster is silent or its control is off', () => {
		expect(stopButton(at({ heater: 50 }, { now: 1_000_000 + STOP_STALE_MS + 1 }))).toBeNull();
		expect(stopButton(at({ heater: 50 }, { reading: null }))).toBeNull();
		expect(stopButton(at({ fan: 100 }, { roastOpen: true, controllable: false }))).toBeNull();
	});
});
