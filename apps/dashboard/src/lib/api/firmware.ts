/** ESP32 firmware with this server's settings filled in (docs/esp32-fluid-bed.md). */

import { BASE_URL, errorMessage, request } from '#lib/api/http.js';

export const firmware = {
	info: () => request<import('#lib/roaster/firmware.js').FirmwareInfo>('/api/firmware'),

	/** The zip saved by the browser; answers its file name and what was left to fill in. */
	download: async (
		req: import('#lib/roaster/firmware.js').FirmwareRequest
	): Promise<{ file: string; missing: string[] }> => {
		const res = await fetch(`${BASE_URL}/api/firmware`, {
			method: 'POST',
			credentials: 'same-origin',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(req)
		});
		if (res.status === 401) {
			window.dispatchEvent(new CustomEvent('auth_required'));
			throw new Error('Authentication required');
		}
		if (!res.ok)
			throw new Error(
				errorMessage(await res.text().catch(() => '')) || `The download failed: ${res.status}`
			);
		const file =
			/filename="([^"]+)"/.exec(res.headers.get('content-disposition') ?? '')?.[1] ??
			'roastwarden-firmware.zip';
		const missing = (res.headers.get('x-firmware-missing') ?? '').split(',').filter(Boolean);
		const url = URL.createObjectURL(await res.blob());
		const a = document.createElement('a');
		a.href = url;
		a.download = file;
		a.click();
		URL.revokeObjectURL(url);
		return { file, missing };
	}
};
