/** Cupping on the SCA form (docs/cupping.md). */

import { request } from '#lib/api/http.js';

interface CuppingAttribute {
	id: string;
	cupping_id: string;
	attribute_name: string;
	score: number;
	created_at: string;
}

/** A roast's cupping on the classic SCA form (docs/cupping.md). */
export interface CuppingScore {
	id: string;
	session_id: string;
	scoring_framework: string;
	/** The ten attributes' total less defects; null until all ten are scored. */
	final_score: number | null;
	total_score: number | null;
	/** Points off: 2 per tainted cup, 4 per faulty cup. */
	defects: number;
	taint_cups: number;
	fault_cups: number;
	band: 'outstanding' | 'excellent' | 'very good' | 'below specialty' | null;
	notes: string | null;
	created_at: string;
	updated_at: string;
	/** In the form's order. */
	attributes: CuppingAttribute[];
}

export interface CreateCuppingRequest {
	scoring_framework?: string;
	notes?: string;
	attributes: { name: string; score: number }[];
	taint_cups?: number;
	fault_cups?: number;
}

export const cupping = {
	get: (sessionId: string) => request<CuppingScore | null>(`/api/sessions/${sessionId}/cupping`),

	create: (sessionId: string, req: CreateCuppingRequest) =>
		request<CuppingScore>(`/api/sessions/${sessionId}/cupping`, {
			method: 'POST',
			body: JSON.stringify(req)
		}),

	delete: (sessionId: string) =>
		request<void>(`/api/sessions/${sessionId}/cupping`, { method: 'DELETE' })
};
