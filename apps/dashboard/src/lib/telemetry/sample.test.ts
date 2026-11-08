import { describe, it, expect } from 'vitest';
import { sampleToTelemetry, hasTemperature, type Sample } from './sample.js';

describe('v2 sample conversion', () => {
	it('maps short keys to the Telemetry shape', () => {
		const s: Sample = {
			q: 12,
			t: 1_700_000_000_500,
			bt: 185.2,
			et: 210.4,
			rb: 8.1,
			re: 3.2,
			rd: 9,
			h: 55.5,
			f: 78.4,
			sp: 190,
			m: 1,
			he: true,
			x: { rssi: -70, Kp: 18.8, ambientTemp: 22.5 }
		};
		const t = sampleToTelemetry(s);
		expect(t.beanTemp).toBe(185.2);
		expect(t.envTemp).toBe(210.4);
		expect(t.rateOfRise).toBe(8.1);
		expect(t.rateOfRiseEt).toBe(3.2);
		expect(t.deviceRateOfRise).toBe(9);
		expect(t.heater).toBe(55.5);
		expect(t.fan).toBe(78.4);
		expect(t.setpoint).toBe(190);
		expect(t.controlMode).toBe(1);
		expect(t.heaterEnable).toBe(1);
		expect(t.rssi).toBe(-70);
		expect(t.Kp).toBe(18.8);
		expect(t.ambientTemp).toBe(22.5);
		expect(t.timestamp).toBe(1_700_000_000);
	});

	it('defaults missing control fields and flags temperature-less samples', () => {
		const s: Sample = { q: 1, t: 0 };
		const t = sampleToTelemetry(s);
		expect(Number.isNaN(t.beanTemp)).toBe(true);
		expect(t.heater).toBe(0);
		expect(t.controlMode).toBe(0);
		expect(t.rateOfRise).toBeUndefined();
		expect(hasTemperature(s)).toBe(false);
		expect(hasTemperature({ q: 1, t: 0, et: 20 })).toBe(true);
	});
});
