import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/beans/${encodeURIComponent(params.id)}`);
	if (res.status === 404) error(404, 'Bean not found');
	if (!res.ok) error(500, 'Failed to load the bean');
	return { bean: await res.json() };
};
