import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url }) => {
	const idsParam = url.searchParams.get('ids') ?? '';
	const ids = idsParam
		.split(',')
		.map((s) => s.trim())
		.filter(Boolean);
	return { ids };
};
