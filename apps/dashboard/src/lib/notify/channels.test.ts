import { describe, expect, it } from 'vitest';
import type { NotificationOverview } from '#lib/api/notifications.js';
import { CHANNELS, channelValues, fromOverview, missing, statusLine } from './channels.js';

const none = { last_ok_at: null, last_error: null, last_error_at: null };
const overview: NotificationOverview = {
	channels: {
		push: { enabled: true, devices: 1, status: none },
		gotify: { enabled: true, url: 'https://g.example.org', token_set: true, status: none },
		ntfy: { enabled: false, url: '', topic: '', token_set: false, status: none },
		matrix: { enabled: false, homeserver: '', room: '', token_set: false, status: none },
		webhook: { enabled: true, url_set: true, status: none }
	},
	events: { backup: true, offline: true, stock: false, offline_minutes: 3 },
	problems: []
};
const ch = (id: string) => CHANNELS.find((c) => c.id === id)!;

describe('notification channels form', () => {
	it('starts secrets empty and remembers which are set', () => {
		const f = fromOverview(overview);
		expect(f.values.notify_gotify_token).toBe('');
		expect(f.isSet.notify_gotify_token).toBe(true);
		expect(f.isSet.notify_webhook_url).toBe(true);
		expect(f.enabled.ntfy).toBe(false);
	});

	it('sends the switch and every field, trimmed, with a kept secret empty', () => {
		const f = fromOverview(overview);
		f.values.notify_gotify_url = ' https://g2.example.org ';
		expect(channelValues(ch('gotify'), f)).toEqual({
			notify_gotify_enabled: 'true',
			notify_gotify_url: 'https://g2.example.org',
			notify_gotify_token: ''
		});
	});

	it('says what a channel still needs; a stored secret counts', () => {
		const f = fromOverview(overview);
		expect(missing(ch('gotify'), f)).toBeNull();
		expect(missing(ch('webhook'), f)).toBeNull();
		expect(missing(ch('ntfy'), f)).toBe('Needs topic');
		expect(missing(ch('matrix'), f)).toBe('Needs homeserver and room id and access token');
	});

	it('reports the later of the last success and the last failure', () => {
		const when = (s: string) => s.slice(11, 16);
		expect(statusLine(none, when)).toBeNull();
		expect(statusLine({ ...none, last_ok_at: '2026-10-24T10:00:00Z' }, when)).toEqual({
			text: 'Last sent 10:00',
			ok: true
		});
		expect(
			statusLine(
				{
					last_ok_at: '2026-10-24T10:00:00Z',
					last_error: 'refused',
					last_error_at: '2026-10-24T11:00:00Z'
				},
				when
			)
		).toEqual({ text: 'Last try failed 11:00: refused', ok: false });
		expect(
			statusLine(
				{
					last_ok_at: '2026-10-24T12:00:00Z',
					last_error: 'refused',
					last_error_at: '2026-10-24T11:00:00Z'
				},
				when
			)?.ok
		).toBe(true);
	});
});
