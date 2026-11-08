import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Brand, SavedDesign } from '#lib/api/labelStudio.js';
import type { LabelOptions } from '#lib/labels/labels.js';
import { TEMPLATE } from '#lib/labels/studio.js';

type RecentRoast = { id: string; name: string; batch_number?: number | null; status?: string };

/** A design to edit: a saved one (its versions too), a built-in or folder one (to start
 *  from), or `new` (`?from=<design>` for its source, else the template). */
export const load: PageLoad = async ({ params, url, fetch }) => {
	const isNew = params.id === 'new';
	const from = url.searchParams.get('from');
	const wanted = isNew ? from : params.id;
	const [designRes, optionsRes, roastsRes, brandsRes] = await Promise.all([
		wanted ? fetch(`/api/label-designs/${encodeURIComponent(wanted)}`) : Promise.resolve(null),
		fetch('/api/labels'),
		fetch('/api/sessions?limit=25'),
		fetch('/api/label-brands')
	]);
	if (designRes && !designRes.ok)
		error(designRes.status === 404 ? 404 : 500, `No design ${wanted}`);
	if (!optionsRes.ok) error(500, 'Failed to load the label choices');
	const design = designRes ? ((await designRes.json()) as SavedDesign) : null;
	const roasts = roastsRes.ok ? ((await roastsRes.json()) as RecentRoast[]) : [];
	return {
		isNew,
		from: from ?? '',
		design,
		source: design?.source ?? TEMPLATE,
		options: (await optionsRes.json()) as LabelOptions,
		roasts: roasts.filter((r) => r.status === 'completed' || r.status === undefined).slice(0, 20),
		brands: brandsRes.ok ? ((await brandsRes.json()) as Brand[]) : []
	};
};
