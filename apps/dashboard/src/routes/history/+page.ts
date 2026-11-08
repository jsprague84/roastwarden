import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/sessions?limit=500');
	if (!res.ok) error(500, 'Failed to load sessions');
	const sessions = await res.json();
	return { sessions };
};
