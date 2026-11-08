import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/device-profiles');
	if (!res.ok) error(500, 'Failed to load device profiles');
	const profiles = await res.json();
	return { profiles };
};
