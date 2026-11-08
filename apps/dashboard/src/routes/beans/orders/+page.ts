import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/orders?status=all');
	if (!res.ok) error(500, 'Failed to load the orders');
	return { orders: await res.json() };
};
