import type { PageLoad } from './$types';
import type { FirmwareInfo } from '#lib/roaster/firmware.js';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/firmware');
	if (!res.ok) return { info: null as FirmwareInfo | null, status: res.status };
	return { info: (await res.json()) as FirmwareInfo, status: res.status };
};
