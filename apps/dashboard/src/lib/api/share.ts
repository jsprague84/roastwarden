/** Share links: a roast's public page (docs/labels.md, "The share page"). */

import { request, tzMinutes } from '#lib/api/http.js';

export interface ShareLink {
	code: string;
	session_id: string;
	name: string;
	batch?: number | null;
	message?: string | null;
	tz_minutes: number;
	created_at: string;
	created_by: string;
	revoked_at?: string | null;
	scans: number;
	last_scan_at?: string | null;
	/** The page's address, `https://…/R/CODE` */
	url: string;
}

export const share = {
	/** A roast's links, newest first, the revoked ones too */
	forRoast: (sessionId: string) =>
		request<ShareLink[]>(`/api/sessions/${encodeURIComponent(sessionId)}/share`),
	/** The roast's newest link (its message replaced when one is given), else a new one;
	 *  `fresh` always makes another */
	make: (sessionId: string, opts: { message?: string; fresh?: boolean } = {}) =>
		request<ShareLink>(`/api/sessions/${encodeURIComponent(sessionId)}/share`, {
			method: 'POST',
			body: JSON.stringify({ message: opts.message, new: opts.fresh ?? false, tz: tzMinutes() })
		}),
	all: (all = false) => request<ShareLink[]>(`/api/share?all=${all}`),
	revoke: (code: string) =>
		request<ShareLink>(`/api/share/${encodeURIComponent(code)}`, { method: 'DELETE' })
};
