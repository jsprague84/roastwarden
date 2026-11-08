/** Bag labels (docs/labels.md): the options, and a roast's sheet as SVG. */

import { BASE_URL, errorMessage, request } from '#lib/api/http.js';

export const labels = {
	options: () => request<import('#lib/labels/labels.js').LabelOptions>('/api/labels'),

	/** The first page as SVG, with each label's outline; an error says what went wrong
	 *  (an owner's design that does not compile says where). */
	preview: async (sessionId: string, query: string, signal?: AbortSignal): Promise<string> => {
		const res = await fetch(
			`${BASE_URL}/api/sessions/${encodeURIComponent(sessionId)}/labels.svg?${query}`,
			{
				credentials: 'same-origin',
				signal
			}
		);
		if (!res.ok)
			throw new Error(
				errorMessage(await res.text().catch(() => '')) || `Labels failed: ${res.status}`
			);
		return res.text();
	}
};
