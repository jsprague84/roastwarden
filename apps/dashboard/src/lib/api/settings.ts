/** The settings table: read, one set, several at once. */

import { request } from '#lib/api/http.js';

export const settings = {
	get: () => request<Record<string, string>>('/api/settings'),

	set: (key: string, value: string) =>
		request<void>(
			`/api/settings/${encodeURIComponent(key)}`,
			{
				method: 'PUT',
				body: JSON.stringify({ value })
			},
			true
		),

	/** Several at once, all or none; a secret sent empty keeps its stored value. */
	setMany: (values: Record<string, string>) =>
		request<{ updated: string[] }>(
			'/api/settings',
			{ method: 'PUT', body: JSON.stringify({ values }) },
			true
		)
};
