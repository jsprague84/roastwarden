/**
 * PDF reports (docs/reports.md): the kinds, and a report fetched for the preview or the download.
 */

import { BASE_URL, errorMessage, request } from '#lib/api/http.js';

export interface ReportKind {
	id: string;
	title: string;
	description: string;
	/** The data's schema (`format=json`) */
	schema: string;
	/** `/api/reports/roast/{id}` */
	path: string;
}

export const reports = {
	kinds: () => request<ReportKind[]>('/api/reports'),

	/** A report as a blob (PDF, or every page as one SVG for a preview) and its page count;
	 *  an error says what went wrong (a design that does not compile says where). */
	fetch: async (
		path: string,
		signal?: AbortSignal
	): Promise<{ blob: Blob; pages: number | null }> => {
		const res = await fetch(`${BASE_URL}${path}`, { credentials: 'same-origin', signal });
		if (res.status === 401) {
			window.dispatchEvent(new CustomEvent('auth_required'));
			throw new Error('Authentication required');
		}
		if (!res.ok)
			throw new Error(
				errorMessage(await res.text().catch(() => '')) || `The report failed: ${res.status}`
			);
		const pages = Number(res.headers.get('x-report-pages'));
		return { blob: await res.blob(), pages: Number.isFinite(pages) && pages > 0 ? pages : null };
	}
};
