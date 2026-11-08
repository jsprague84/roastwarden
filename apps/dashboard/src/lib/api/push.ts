/** Web Push (docs/push.md; Settings → This browser). */

import { request } from '#lib/api/http.js';

export interface PushSubscriptionRow {
	id: number;
	name: string;
	/** The push service's host. */
	service: string;
	created_at: string;
	last_ok_at: string | null;
	last_error: string | null;
}

export interface PushDelivery {
	id: number;
	name: string;
	ok: boolean;
	detail: string;
	removed: boolean;
}

export const push = {
	key: () => request<{ public_key: string }>('/api/push/key'),
	list: () => request<PushSubscriptionRow[]>('/api/push/subscriptions'),
	subscribe: (sub: PushSubscriptionJSON, name: string) =>
		request<PushSubscriptionRow>('/api/push/subscriptions', {
			method: 'POST',
			body: JSON.stringify({ ...sub, name })
		}),
	unsubscribe: (endpoint: string) =>
		request<void>('/api/push/unsubscribe', { method: 'POST', body: JSON.stringify({ endpoint }) }),
	remove: (id: number) => request<void>(`/api/push/subscriptions/${id}`, { method: 'DELETE' }),
	test: (id?: number) =>
		request<PushDelivery[]>('/api/push/test', {
			method: 'POST',
			body: JSON.stringify(id ? { id } : {})
		})
};
