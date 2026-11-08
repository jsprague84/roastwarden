import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

// The section's old URLs: /sessions/... is now /history/...
export const load: PageLoad = ({ params, url }) => {
	redirect(308, `/history${params.rest ? `/${params.rest}` : ''}${url.search}`);
};
