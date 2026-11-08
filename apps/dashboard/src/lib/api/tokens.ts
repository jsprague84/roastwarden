/** API tokens (SECURITY.md; Settings → Tokens), and which roasters tokens may control. */

import type { TokenPreset } from '#lib/api/auth.js';
import { request } from '#lib/api/http.js';

export interface ApiToken {
	id: number;
	name: string;
	/** The first characters of the secret, to tell tokens apart. */
	prefix: string;
	preset: TokenPreset;
	created_at: string;
	expires_at: string | null;
	last_used_at: string | null;
	last_used_ip: string | null;
	revoked_at: string | null;
	/** Starting a roast on a physical roaster and deletes wait for the owner's approval. */
	require_approval: boolean;
}

export interface RoasterTokenAccess {
	device_id: string;
	tokens_may_control: boolean;
	/** The virtual roaster: tokens may always control it. */
	always: boolean;
	updated_at: string | null;
}

export const tokens = {
	list: () => request<ApiToken[]>('/api/tokens'),
	create: (
		name: string,
		preset: TokenPreset,
		expiresInDays: number | null,
		requireApproval = true
	) =>
		request<{ token: ApiToken; secret: string }>('/api/tokens', {
			method: 'POST',
			body: JSON.stringify({
				name,
				preset,
				expires_in_days: expiresInDays,
				require_approval: requireApproval
			})
		}),
	setRequireApproval: (id: number, on: boolean) =>
		request<ApiToken>(`/api/tokens/${id}`, {
			method: 'PUT',
			body: JSON.stringify({ require_approval: on })
		}),
	revoke: (id: number) => request<ApiToken>(`/api/tokens/${id}/revoke`, { method: 'POST' }),
	remove: (id: number) => request<void>(`/api/tokens/${id}`, { method: 'DELETE' }),
	access: (deviceId: string) =>
		request<RoasterTokenAccess>(`/api/roaster/${encodeURIComponent(deviceId)}/token-access`),
	setAccess: (deviceId: string, allowed: boolean) =>
		request<RoasterTokenAccess>(`/api/roaster/${encodeURIComponent(deviceId)}/token-access`, {
			method: 'PUT',
			body: JSON.stringify({ tokens_may_control: allowed })
		})
};
