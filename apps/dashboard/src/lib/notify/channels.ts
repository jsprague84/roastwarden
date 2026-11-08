/**
 * Settings → Notifications (docs/notifications.md): the channels, their fields, and the
 * mapping between the server's overview and the form. Secrets come back only as "set";
 * an empty secret in the form means "keep the stored one".
 */
import type { ChannelStatus, NotificationOverview, NotifyChannel } from '#lib/api/notifications.js';

interface Field {
	key: string;
	label: string;
	placeholder?: string;
	secret?: boolean;
	type?: 'url' | 'text';
}

export interface ChannelDef {
	id: Exclude<NotifyChannel, 'push'>;
	name: string;
	blurb: string;
	enabledKey: string;
	fields: Field[];
}

export const CHANNELS: ChannelDef[] = [
	{
		id: 'gotify',
		name: 'Gotify',
		blurb:
			'A self-hosted push server (Gotify or rstify) and its phone app. Make an application on the server and paste its token.',
		enabledKey: 'notify_gotify_enabled',
		fields: [
			{
				key: 'notify_gotify_url',
				label: 'Server URL',
				placeholder: 'https://gotify.example.org',
				type: 'url'
			},
			{ key: 'notify_gotify_token', label: 'Application token', secret: true }
		]
	},
	{
		id: 'ntfy',
		name: 'ntfy',
		blurb:
			'Publish to a topic and subscribe to it in the ntfy app. On a public server anyone who knows the topic can read it: use a long random name, or an access token.',
		enabledKey: 'notify_ntfy_enabled',
		fields: [
			{ key: 'notify_ntfy_url', label: 'Server URL', placeholder: 'https://ntfy.sh', type: 'url' },
			{ key: 'notify_ntfy_topic', label: 'Topic', placeholder: 'roastwarden-7f3k2q' },
			{ key: 'notify_ntfy_token', label: 'Access token (optional)', secret: true }
		]
	},
	{
		id: 'matrix',
		name: 'Matrix',
		blurb:
			'A message in a Matrix room, from an account you invite to it (a bot account is best). The room id is under Room settings → Advanced.',
		enabledKey: 'notify_matrix_enabled',
		fields: [
			{
				key: 'notify_matrix_homeserver',
				label: 'Homeserver',
				placeholder: 'https://matrix.example.org',
				type: 'url'
			},
			{
				key: 'notify_matrix_room',
				label: 'Room id',
				placeholder: '!abc123:example.org or !abc… (newer rooms)'
			},
			{ key: 'notify_matrix_token', label: 'Access token', secret: true }
		]
	},
	{
		id: 'webhook',
		name: 'Webhook',
		blurb:
			'A JSON POST for every alarm and system event, to an n8n workflow say. The URL is kept like a password: it usually is one.',
		enabledKey: 'notify_webhook_enabled',
		fields: [
			{
				key: 'notify_webhook_url',
				label: 'URL',
				placeholder: 'https://n8n.example.org/webhook/roast',
				secret: true,
				type: 'url'
			}
		]
	}
];

export interface FormState {
	enabled: Record<string, boolean>;
	/** Field values; secrets start empty. */
	values: Record<string, string>;
	/** Which secrets the server has. */
	isSet: Record<string, boolean>;
}

export function fromOverview(o: NotificationOverview): FormState {
	const c = o.channels;
	return {
		enabled: {
			gotify: c.gotify.enabled,
			ntfy: c.ntfy.enabled,
			matrix: c.matrix.enabled,
			webhook: c.webhook.enabled
		},
		values: {
			notify_gotify_url: c.gotify.url,
			notify_gotify_token: '',
			notify_ntfy_url: c.ntfy.url,
			notify_ntfy_topic: c.ntfy.topic,
			notify_ntfy_token: '',
			notify_matrix_homeserver: c.matrix.homeserver,
			notify_matrix_room: c.matrix.room,
			notify_matrix_token: '',
			notify_webhook_url: ''
		},
		isSet: {
			notify_gotify_token: c.gotify.token_set,
			notify_ntfy_token: c.ntfy.token_set,
			notify_matrix_token: c.matrix.token_set,
			notify_webhook_url: c.webhook.url_set
		}
	};
}

/** What Save (or Test) sends for one channel: every field trimmed, the switch as text. */
export function channelValues(ch: ChannelDef, f: FormState): Record<string, string> {
	const out: Record<string, string> = { [ch.enabledKey]: String(!!f.enabled[ch.id]) };
	for (const field of ch.fields) out[field.key] = (f.values[field.key] ?? '').trim();
	return out;
}

/** What the channel still needs before it can send, or null. */
export function missing(ch: ChannelDef, f: FormState): string | null {
	const has = (k: string) => (f.values[k] ?? '').trim() !== '' || !!f.isSet[k];
	const need = ch.fields.filter(
		(x) => !x.label.includes('optional') && x.key !== 'notify_ntfy_url' && !has(x.key)
	);
	return need.length ? `Needs ${need.map((x) => x.label.toLowerCase()).join(' and ')}` : null;
}

/** One line on how the channel's last send went. */
export function statusLine(
	s: ChannelStatus | undefined,
	when: (iso: string) => string
): { text: string; ok: boolean } | null {
	if (!s) return null;
	const ok = s.last_ok_at ? Date.parse(s.last_ok_at) : -Infinity;
	const bad = s.last_error_at ? Date.parse(s.last_error_at) : -Infinity;
	if (bad > ok && s.last_error)
		return { text: `Last try failed ${when(s.last_error_at!)}: ${s.last_error}`, ok: false };
	if (s.last_ok_at) return { text: `Last sent ${when(s.last_ok_at)}`, ok: true };
	return null;
}
