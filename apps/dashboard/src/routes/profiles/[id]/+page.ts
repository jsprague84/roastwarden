import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { ProfileWithPoints } from '#lib/api/profiles.js';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/profiles/${encodeURIComponent(params.id)}`);
	if (res.status === 404) error(404, 'Profile not found');
	if (!res.ok) error(500, 'Failed to load profile');
	const profile: ProfileWithPoints = await res.json();
	return { profile };
};
