import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/profiles?include_private=true');
	if (!res.ok) error(500, 'Failed to load profiles');
	const profiles = await res.json();
	return { profiles };
};
