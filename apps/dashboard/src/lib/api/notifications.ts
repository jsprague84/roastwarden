/**
 * Notifications (docs/notifications.md; Settings → Notifications): the channels, the system
 * events, a test.
 */

import { request } from '#lib/api/http.js';

export interface ChannelStatus {
	last_ok_at: string | null;
	last_error: string | null;
	last_error_at: string | null;
}

export interface SystemProblem {
	key: string;
	title: string;
	body: string;
	changed_at: string;
}

export interface NotificationOverview {
	channels: {
		push: { enabled: true; devices: number; status: ChannelStatus };
		gotify: { enabled: boolean; url: string; token_set: boolean; status: ChannelStatus };
		ntfy: {
			enabled: boolean;
			url: string;
			topic: string;
			token_set: boolean;
			status: ChannelStatus;
		};
		matrix: {
			enabled: boolean;
			homeserver: string;
			room: string;
			token_set: boolean;
			status: ChannelStatus;
		};
		webhook: { enabled: boolean; url_set: boolean; status: ChannelStatus };
	};
	events: { backup: boolean; offline: boolean; stock: boolean; offline_minutes: number };
	problems: SystemProblem[];
}

export type NotifyChannel = 'push' | 'gotify' | 'ntfy' | 'matrix' | 'webhook';

export const notifyApi = {
	overview: () => request<NotificationOverview>('/api/notifications'),
	/** A test through one channel with the form's unsaved values (empty secret = stored). */
	test: (channel: NotifyChannel, values: Record<string, string> = {}) =>
		request<{ message: string }>(
			'/api/notifications/test',
			{ method: 'POST', body: JSON.stringify({ channel, values }) },
			true
		)
};
