import { describe, expect, it } from 'vitest';
import { fromStatus, problem, statusLine, values } from './form.js';
import type { HaStatus } from '#lib/api/homeAssistant.js';

const status = (over: Partial<HaStatus> = {}): HaStatus => ({
	enabled: true,
	broker: 'server',
	username: null,
	password_set: false,
	connected: true,
	prefix: 'homeassistant',
	include_virtual: false,
	interval_seconds: 5,
	built_in_user: null,
	built_in_without_user: false,
	roasters: [],
	waiting: [],
	last_published_at: null,
	problem: null,
	...over
});

describe('Home Assistant form', () => {
	it('reads the server broker or another one', () => {
		expect(fromStatus(status(), {}).where).toBe('server');
		const f = fromStatus(status({ broker: 'ha.lan:1883', username: 'rr' }), {
			ha_broker: 'mqtt://ha.lan'
		});
		expect(f.where).toBe('other');
		expect(f.broker).toBe('mqtt://ha.lan');
		expect(f.username).toBe('rr');
		expect(f.password).toBe('');
	});

	it('clears the broker and its user for the server broker; keeps an untyped password', () => {
		const f = { ...fromStatus(status(), {}), username: 'left over', password: 'x' };
		const v = values(f);
		expect(v.ha_broker).toBe('');
		expect(v.ha_username).toBe('');
		expect(v.ha_password).toBe('');
		const o = values({
			...f,
			where: 'other',
			broker: ' mqtt://ha.lan:1884 ',
			username: 'rr',
			password: ''
		});
		expect(o.ha_broker).toBe('mqtt://ha.lan:1884');
		expect(o.ha_password).toBe('');
		expect(o.ha_enabled).toBe('true');
	});

	it('checks the broker, prefix and interval', () => {
		const f = fromStatus(status(), {});
		expect(problem(f)).toBeNull();
		expect(problem({ ...f, where: 'other', broker: 'mqtt://' })).toMatch(/broker/);
		expect(problem({ ...f, where: 'other', broker: 'mqtts://ha.lan' })).toMatch(/broker/);
		expect(problem({ ...f, where: 'other', broker: 'mqtt://ha.lan:1883' })).toBeNull();
		expect(problem({ ...f, prefix: 'ha/#' })).toMatch(/prefix/);
		expect(problem({ ...f, interval: 0 })).toMatch(/interval/);
	});

	it('says what is published', () => {
		expect(statusLine(status({ enabled: false })).text).toMatch(/Off/);
		expect(statusLine(status({ problem: 'not connected to ha.lan:1883 yet' })).ok).toBe(false);
		const r = {
			device_id: 'r',
			name: 'R',
			available: true,
			config_topic: '',
			state_topic: '',
			entities: []
		};
		expect(statusLine(status({ roasters: [r] })).text).toBe(
			"Publishing to the server's broker: 1 roaster."
		);
		expect(statusLine(status({ waiting: ['virtual_roaster'] })).text).toBe(
			"Publishing to the server's broker: no roaster. Not yet: virtual_roaster, waiting to be set up under Roasters."
		);
	});
});
