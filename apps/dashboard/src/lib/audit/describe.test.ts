import { describe as group, it, expect } from 'vitest';
import { describe, who, clientName } from './describe.js';

group('audit describe', () => {
	it('names control commands with their value', () => {
		expect(
			describe({
				method: 'POST',
				path: '/api/roaster/esp32_roaster_01/control/setpoint',
				body: '{"value":205}'
			})
		).toBe('Setpoint 205');
		expect(
			describe({
				method: 'POST',
				path: '/api/roaster/x/control/heater_enable',
				body: '{"enabled":false}'
			})
		).toBe('Heater power off');
		expect(
			describe({ method: 'POST', path: '/api/roaster/x/control/fan', body: '{"percent":72}' })
		).toBe('Fan 72 %');
		expect(
			describe({
				method: 'POST',
				path: '/api/roaster/x/control/pid',
				body: '{"kp":10,"ki":0.1,"kd":60}'
			})
		).toBe('PID gains Kp 10 · Ki 0.1 · Kd 60');
		expect(
			describe({ method: 'POST', path: '/api/roaster/x/control/emergency_stop', body: null })
		).toBe('Emergency stop');
		expect(describe({ method: 'POST', path: '/api/roaster/x/control/newthing', body: null })).toBe(
			'Control newthing'
		);
	});

	it('names roast actions, events and profile drive', () => {
		expect(describe({ method: 'POST', path: '/api/sessions/abc/start', body: null })).toBe(
			'Start roast'
		);
		expect(
			describe({
				method: 'POST',
				path: '/api/sessions/abc/events',
				body: '{"event_type":"charge"}'
			})
		).toBe('Mark event CHARGE');
		expect(describe({ method: 'DELETE', path: '/api/sessions/abc/replay', body: null })).toBe(
			'Stop profile drive'
		);
		expect(describe({ method: 'POST', path: '/api/sessions', body: '{"name":"Batch 2"}' })).toBe(
			'New roast “Batch 2”'
		);
		expect(describe({ method: 'DELETE', path: '/api/sessions/abc', body: null })).toBe(
			'Delete roast'
		);
	});

	it('names settings and falls back to the path', () => {
		expect(
			describe({ method: 'PUT', path: '/api/settings/backup_keep', body: '{"value":"14"}' })
		).toBe('Setting backup_keep = 14');
		const long = describe({
			method: 'PUT',
			path: '/api/settings/roast_alarms',
			body: JSON.stringify({ value: '[' + '{"name":"x"},'.repeat(20) + ']' })
		});
		expect(long.length).toBeLessThan(100);
		expect(long.endsWith('…')).toBe(true);
		expect(describe({ method: 'POST', path: '/api/backups', body: null })).toBe('POST /backups');
		expect(describe({ method: 'POST', path: '/api/virtual-roaster/dump', body: null })).toBe(
			'Simulator: dump beans'
		);
	});

	it('says who', () => {
		expect(who({ actor: 'owner key', token_name: null, remote_user: 'me@x' })).toBe(
			'me@x · owner key'
		);
		expect(who({ actor: 'none', token_name: null, remote_user: null })).toBe('no key');
		expect(who({ actor: 'token', token_name: 'agent', remote_user: null })).toBe('token “agent”');
		expect(who({ actor: 'none', token_name: 'agent', remote_user: null })).toBe(
			'token “agent” (no longer valid)'
		);
	});

	it('names token management and the per-roaster switch', () => {
		expect(
			describe({
				method: 'POST',
				path: '/api/tokens',
				body: '{"name":"agent","preset":"read_only"}'
			})
		).toBe('New token “agent” (read-only)');
		expect(describe({ method: 'POST', path: '/api/tokens/3/revoke', body: null })).toBe(
			'Revoke token'
		);
		expect(describe({ method: 'DELETE', path: '/api/tokens/3', body: null })).toBe('Remove token');
		expect(
			describe({ method: 'PUT', path: '/api/tokens/3', body: '{"require_approval":true}' })
		).toBe('Token asks first: on');
		expect(describe({ method: 'POST', path: '/api/approvals/Zx9/approve', body: null })).toBe(
			'Approve a request'
		);
		expect(describe({ method: 'POST', path: '/api/approvals/Zx9/deny', body: null })).toBe(
			'Deny a request'
		);
		expect(
			describe({
				method: 'PUT',
				path: '/api/roaster/esp32_roaster_01/token-access',
				body: '{"tokens_may_control":true}'
			})
		).toBe('Tokens may control this roaster: on');
	});

	it('shortens user agents', () => {
		expect(
			clientName(
				'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1'
			)
		).toBe('Safari on iOS');
		expect(
			clientName(
				'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36'
			)
		).toBe('Chrome on Linux');
		expect(clientName('curl/8.10.1')).toBe('curl');
		expect(clientName(null)).toBe('');
	});
});
