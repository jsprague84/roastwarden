import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch, params }) => {
	const res = await fetch(`/api/orders/${encodeURIComponent(params.id)}`);
	if (res.status === 404) error(404, 'Order not found');
	if (!res.ok) error(500, 'Failed to load the order');
	return { order: await res.json() };
};
