/** Approvals (docs/control.md, "Approvals"): a token's call held for the owner. */

import { request } from '#lib/api/http.js';

type ApprovalStatus = 'pending' | 'approved' | 'done' | 'failed' | 'denied' | 'expired';

/** A token's call held for the owner: starting a roast on a physical roaster, or a delete. */
export interface Approval {
	id: string;
	status: ApprovalStatus;
	/** roast.start, roast.delete, bean.delete, profile.delete, roaster.delete, label_design.delete, label_asset.delete, label_brand.delete, labels.print, labels.run.print, label_printer.delete or roaster.control */
	action: string;
	summary: string;
	method: string;
	path: string;
	target_id: string;
	target_name: string | null;
	device_id: string | null;
	token_id: number | null;
	token_name: string;
	client_ip: string | null;
	url: string;
	created_at: string;
	expires_at: string;
	decided_at: string | null;
	result_status: number | null;
	result: unknown;
}

export const approvals = {
	list: () => request<Approval[]>('/api/approvals'),
	approve: (id: string) =>
		request<Approval>(`/api/approvals/${encodeURIComponent(id)}/approve`, { method: 'POST' }),
	deny: (id: string) =>
		request<Approval>(`/api/approvals/${encodeURIComponent(id)}/deny`, { method: 'POST' })
};
