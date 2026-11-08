import { describe, expect, it } from 'vitest';
import {
	passwordProblem,
	roasterIdProblem,
	sourceNote,
	taken,
	type MqttPassword
} from './mqttPasswords.js';

describe('MQTT passwords', () => {
	it('checks an id as the server does', () => {
		expect(roasterIdProblem('garage_logger')).toBeNull();
		expect(roasterIdProblem('drum-2')).toBeNull();
		expect(roasterIdProblem('')).toBe('Give the roaster an id');
		expect(roasterIdProblem('a/b')).toMatch(/Letters/);
		expect(roasterIdProblem('x'.repeat(49))).toMatch(/at most 48/);
		expect(roasterIdProblem('roastwarden')).toMatch(/broker's own/);
	});

	it('checks a password of your own; empty is a random one', () => {
		expect(passwordProblem('')).toBeNull();
		expect(passwordProblem('abcdefgh')).toBeNull();
		expect(passwordProblem('S3cret!#%&*+=?.,:;')).toBeNull();
		expect(passwordProblem('short')).toBe('8 to 128 characters');
		expect(passwordProblem('has space1')).toMatch(/No spaces/);
		expect(passwordProblem('quo"te1234')).toMatch(/quotes/);
		expect(passwordProblem('back\\slash1')).toMatch(/backslashes/);
	});

	it('says where a password lives', () => {
		const date = (iso: string) => iso.slice(0, 10);
		const row = (p: Partial<MqttPassword>): MqttPassword => ({
			roaster_id: 'r',
			source: 'dashboard',
			signed_in: false,
			added: false,
			...p
		});
		expect(sourceNote(row({ source: 'env' }), date)).toBe('In .env (MQTT_DEVICES)');
		expect(
			sourceNote(
				row({ created_at: '2026-11-05T10:00:00Z', changed_at: '2026-11-05T10:00:00Z' }),
				date
			)
		).toBe('Added 2026-11-05');
		expect(
			sourceNote(
				row({ created_at: '2026-11-05T10:00:00Z', changed_at: '2026-11-06T09:00:00Z' }),
				date
			)
		).toBe('Added 2026-11-05, new password 2026-11-06');
	});

	it('knows the ids taken', () => {
		const list = {
			built_in_broker: true,
			passwords: [{ roaster_id: 'a', source: 'env' as const, signed_in: false, added: true }]
		};
		expect(taken(list, ' a ')).toBe(true);
		expect(taken(list, 'b')).toBe(false);
		expect(taken(null, 'a')).toBe(false);
	});
});
