import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const [devicesRes, discoveredRes, mdnsRes] = await Promise.all([
		fetch('/api/devices'),
		fetch('/api/devices/discovered'),
		fetch('/api/devices/mdns')
	]);

	if (!devicesRes.ok) error(500, 'Failed to load devices');
	const devices = await devicesRes.json();
	const discovered = discoveredRes.ok ? await discoveredRes.json() : [];
	const mdns = mdnsRes.ok ? await mdnsRes.json() : { enabled: false, services: [] };

	return { devices, discovered, mdns };
};
