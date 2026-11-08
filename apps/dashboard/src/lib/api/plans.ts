/** The planned roasts' queue (docs/batches.md). */

import { request } from '#lib/api/http.js';

export interface PlannedRoast {
	id: string;
	position: number;
	device_id: string | null;
	bean_id: string | null;
	profile_id: string | null;
	green_weight_g: number | null;
	name: string | null;
	notes: string | null;
	/** The roast that ran it; null = still planned. */
	session_id: string | null;
	created_at: string;
	updated_at: string;
	bean_name: string | null;
	bean_stock_g: number | null;
	profile_name: string | null;
	batch_number: number | null;
}

export interface PlanRequest {
	device_id?: string | null;
	bean_id?: string | null;
	profile_id?: string | null;
	green_weight_g?: number | null;
	name?: string | null;
	notes?: string | null;
	position?: number;
}

export const plan = {
	/** The queue in order; deviceId keeps that roaster's plans and the any-roaster ones. */
	list: (deviceId?: string | null, done = false) => {
		const q = new URLSearchParams();
		if (deviceId) q.set('device_id', deviceId);
		if (done) q.set('done', 'true');
		const qs = q.toString();
		return request<PlannedRoast[]>(`/api/plan${qs ? `?${qs}` : ''}`);
	},
	create: (req: PlanRequest) =>
		request<PlannedRoast>('/api/plan', { method: 'POST', body: JSON.stringify(req) }),
	update: (id: string, req: PlanRequest) =>
		request<PlannedRoast>(`/api/plan/${encodeURIComponent(id)}`, {
			method: 'PUT',
			body: JSON.stringify(req)
		}),
	delete: (id: string) =>
		request<void>(`/api/plan/${encodeURIComponent(id)}`, { method: 'DELETE' }),
	reorder: (ids: string[]) =>
		request<PlannedRoast[]>('/api/plan/reorder', { method: 'POST', body: JSON.stringify({ ids }) })
};
