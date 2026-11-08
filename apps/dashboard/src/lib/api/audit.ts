/** The audit log (SECURITY.md; Settings → Audit). */

import type { AuditEntry } from '#lib/audit/describe.js';
import { request } from '#lib/api/http.js';

export interface AuditQuery {
	limit?: number;
	before?: number;
	device_id?: string;
	/** Kinds, any of them (`AUDIT_KINDS`) */
	kinds?: string[];
	/** Text anywhere in the path, body, token, user, roaster, address or client */
	q?: string;
	/** A token's name */
	token?: string;
	/** Only refused or failed requests */
	failed?: boolean;
}

export const audit = {
	list: (q: AuditQuery = {}) => {
		const p = new URLSearchParams();
		if (q.limit) p.set('limit', String(q.limit));
		if (q.before) p.set('before', String(q.before));
		if (q.device_id) p.set('device_id', q.device_id);
		if (q.kinds?.length) p.set('kind', q.kinds.join(','));
		if (q.q?.trim()) p.set('q', q.q.trim());
		if (q.token) p.set('token', q.token);
		if (q.failed) p.set('failed', 'true');
		const qs = p.toString();
		return request<AuditEntry[]>(`/api/audit${qs ? `?${qs}` : ''}`);
	}
};
