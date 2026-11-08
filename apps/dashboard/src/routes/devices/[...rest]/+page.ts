import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import { legacyRoasterPath } from '#lib/routes.js';

// The section's old URLs: /devices/... is now /roasters/...
export const load: PageLoad = ({ params, url }) => {
	redirect(308, legacyRoasterPath(params.rest, url.searchParams));
};
