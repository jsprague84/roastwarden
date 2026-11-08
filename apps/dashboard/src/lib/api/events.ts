/** A roast's landmarks (events). */

import { request } from '#lib/api/http.js';

export interface RoastEvent {
	id: string;
	session_id: string;
	event_type: string;
	elapsed_seconds: number;
	temperature: number | null;
	notes: string | null;
	created_at: string;
}

export interface CreateRoastEventRequest {
	event_type: string;
	elapsed_seconds: number;
	temperature?: number;
	notes?: string;
}

export const events = {
	list: (sessionId: string) => request<RoastEvent[]>(`/api/sessions/${sessionId}/events`),

	create: (sessionId: string, req: CreateRoastEventRequest) =>
		request<RoastEvent>(`/api/sessions/${sessionId}/events`, {
			method: 'POST',
			body: JSON.stringify(req)
		}),

	delete: (sessionId: string, eventId: string) =>
		request<void>(`/api/sessions/${sessionId}/events/${eventId}`, { method: 'DELETE' })
};
