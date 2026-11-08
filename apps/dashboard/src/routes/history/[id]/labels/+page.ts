import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Brand } from '#lib/api/labelStudio.js';
import type { RoastedLot } from '#lib/api/roasted.js';
import type { RoastSession } from '#lib/api/sessions.js';

type BeanWithLabel = {
	id: string;
	name: string;
	label_design?: string | null;
	label_params?: Record<string, unknown> | null;
	label_brand?: string | null;
};
import type { LabelOptions } from '#lib/labels/labels.js';

export const load: PageLoad = async ({ params, fetch }) => {
	const [sessionRes, optionsRes] = await Promise.all([
		fetch(`/api/sessions/${params.id}`),
		fetch('/api/labels')
	]);
	if (!sessionRes.ok) error(sessionRes.status === 404 ? 404 : 500, 'Failed to load the roast');
	if (!optionsRes.ok) error(500, 'Failed to load the label choices');
	const session = (await sessionRes.json()) as RoastSession;
	const options = (await optionsRes.json()) as LabelOptions;
	// The coffee's own label and the brand kits: a choice to start from, not required
	// What is left of it: the print may take its bags off (docs/roasted.md, "Packed bags")
	const tz = -new Date().getTimezoneOffset();
	const [bean, brands, lot] = await Promise.all([
		session.bean_id
			? fetch(`/api/beans/${session.bean_id}`)
					.then((r) => (r.ok ? r.json() : null))
					.catch(() => null)
			: null,
		fetch('/api/label-brands')
			.then((r) => (r.ok ? r.json() : []))
			.catch(() => []),
		fetch(`/api/roasted/${params.id}?tz=${tz}`)
			.then((r) => (r.ok ? r.json() : null))
			.catch(() => null)
	]);
	return {
		session,
		options,
		bean: bean as BeanWithLabel | null,
		brands: brands as Brand[],
		lot: lot as RoastedLot | null
	};
};
