import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/profiles/${params.id}`);
	if (!res.ok) error(500, 'Failed to load profile');
	const profile = await res.json();
	return { profile };
};
