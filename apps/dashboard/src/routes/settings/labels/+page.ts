import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

// Settings → Labels is its own section now, Label Studio
export const load: PageLoad = () => {
	redirect(308, '/labels');
};
