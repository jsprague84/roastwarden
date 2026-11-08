import { describe, expect, it } from 'vitest';
import { blankEvent, describeEvent, eventTimeOnCurve, toInput, validateEvent } from './events.js';
import type { ProfileEventInput } from '#lib/api/profiles.js';

const ev = (o: Partial<ProfileEventInput>): ProfileEventInput => ({ ...blankEvent(), ...o });

describe('validateEvent', () => {
	it('accepts a blank fan event', () => {
		expect(validateEvent(blankEvent(30))).toBeNull();
	});
	it('checks ranges per action', () => {
		expect(validateEvent(ev({ value: 140 }))).toMatch(/0–100/);
		expect(validateEvent(ev({ action: 'setpoint', value: 320 }))).toMatch(/setpoint/);
		expect(validateEvent(ev({ action: 'mode', label: 'turbo' }))).toMatch(/auto or manual/);
		expect(validateEvent(ev({ action: 'landmark', label: 'charge' }))).toMatch(/landmark/);
		expect(validateEvent(ev({ action: 'landmark', label: 'first_crack_start' }))).toBeNull();
		expect(validateEvent(ev({ action: 'note', label: '  ' }))).toMatch(/text/);
	});
	it('needs the reference its trigger uses', () => {
		expect(validateEvent(ev({ trigger_kind: 'bt', at_bt: null }))).toMatch(/BT/);
		expect(validateEvent(ev({ trigger_kind: 'bt', at_bt: 160 }))).toBeNull();
		expect(validateEvent(ev({ at_seconds: -1 }))).toMatch(/time/);
	});
});

describe('describeEvent', () => {
	it('labels each action', () => {
		expect(describeEvent(ev({ value: 60 }))).toBe('Fan 60%');
		expect(describeEvent(ev({ action: 'landmark', label: 'first_crack_start' }))).toBe('FC Start');
		expect(describeEvent(ev({ action: 'setpoint', value: 200 }), (c) => `${c * 2}X`)).toBe(
			'SP 400X'
		);
		expect(describeEvent(ev({ action: 'mode', label: 'manual' }))).toBe('Manual');
	});
});

describe('toInput', () => {
	it('drops the label slot for numeric actions only when blank, and value for text actions', () => {
		const withMeta = {
			...ev({ action: 'note', value: 5, label: 'stir' }),
			id: 'x',
			profile_id: 'p',
			created_at: ''
		};
		expect(toInput(withMeta)).toEqual({
			trigger_kind: 'time',
			at_seconds: 0,
			at_bt: null,
			action: 'note',
			value: null,
			label: 'stir'
		});
		expect(toInput(ev({ label: ' ' })).label).toBeNull();
	});
});

describe('eventTimeOnCurve', () => {
	// Charge 200, TP 100 at 60 s, then +0.25 °C/s.
	const curve: [number, number][] = [
		[0, 200],
		[60, 100],
		[460, 200]
	];
	it('returns the time for time triggers', () => {
		expect(eventTimeOnCurve(ev({ at_seconds: 42 }), curve)).toBe(42);
	});
	it('finds the BT crossing after the turning point, not the drop after charge', () => {
		expect(eventTimeOnCurve(ev({ trigger_kind: 'bt', at_bt: 150 }), curve)).toBeCloseTo(260);
	});
	it('is null when BT never gets there', () => {
		expect(eventTimeOnCurve(ev({ trigger_kind: 'bt', at_bt: 250 }), curve)).toBeNull();
	});
});
