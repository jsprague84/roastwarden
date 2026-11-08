import { describe, expect, it } from 'vitest';
import { agentSetup, ago, browserName, presetLabel, tokenState } from './state.js';

const now = Date.parse('2026-10-24T12:00:00Z');

describe('tokenState', () => {
	it('puts revocation before expiry', () => {
		expect(
			tokenState({ revoked_at: '2026-10-01T00:00:00Z', expires_at: '2027-01-01T00:00:00Z' }, now)
		).toEqual({ kind: 'revoked', text: 'Revoked' });
	});
	it('says when it expires', () => {
		expect(tokenState({ revoked_at: null, expires_at: null }, now).text).toBe('Never expires');
		expect(tokenState({ revoked_at: null, expires_at: '2026-11-23T12:00:00Z' }, now).text).toBe(
			'Expires in 30 days'
		);
		expect(tokenState({ revoked_at: null, expires_at: '2026-10-25T12:00:00Z' }, now).text).toBe(
			'Expires in 1 day'
		);
		expect(tokenState({ revoked_at: null, expires_at: '2026-10-24T11:00:00Z' }, now).kind).toBe(
			'expired'
		);
	});
});

describe('ago', () => {
	it('reads as a short time', () => {
		expect(ago(null, now)).toBe('never');
		expect(ago('2026-10-24T11:59:30Z', now)).toBe('just now');
		expect(ago('2026-10-24T11:30:00Z', now)).toBe('30 min ago');
		expect(ago('2026-10-24T06:00:00Z', now)).toBe('6 h ago');
		expect(ago('2026-10-20T12:00:00Z', now)).toBe('4 days ago');
	});
	it('labels presets', () => {
		expect(presetLabel('read_only')).toBe('Read-only');
		expect(presetLabel('inventory')).toBe('Inventory');
		expect(presetLabel('other')).toBe('other');
	});
});

describe('browserName', () => {
	it('names the browser and its system', () => {
		expect(
			browserName('Mozilla/5.0 (X11; Linux x86_64; rv:142.0) Gecko/20100101 Firefox/142.0')
		).toBe('Firefox on Linux');
		expect(
			browserName(
				'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1'
			)
		).toBe('Safari on iPhone');
		expect(
			browserName(
				'Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Mobile Safari/537.36'
			)
		).toBe('Chrome on Android');
		expect(browserName(null)).toBe('A browser');
		expect(browserName('curl/8.0')).toBe('A browser');
	});
});

describe('agentSetup', () => {
	it('logs in to this server, installs the skill, then checks', () => {
		const lines = agentSetup('https://roast.example/').split('\n');
		expect(lines[0]).toMatch(/^roast login https:\/\/roast\.example /);
		expect(lines[1]).toMatch(/^roast skill install claude-code/);
		expect(lines[2]).toMatch(/^roast status/);
	});
});
