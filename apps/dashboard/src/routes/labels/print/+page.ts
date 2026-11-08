import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { LabelOptions } from '#lib/labels/labels.js';
import type { Brand } from '#lib/api/labelStudio.js';

export type RecentRoast = {
	id: string;
	name: string;
	status: string;
	batch_number?: number | null;
	bean_id?: string | null;
	start_time: string | null;
	end_time: string | null;
	roasted_weight?: number | null;
};

/** A print run: the sheets and designs to choose from, recent roasts to put on the sheet,
 *  and roasts asked for in the address (`?roast=a&roast=b`, from History). */
export const load: PageLoad = async ({ fetch, url }) => {
	const [optionsRes, roastsRes, beansRes, brandsRes] = await Promise.all([
		fetch('/api/labels'),
		fetch('/api/sessions?limit=60'),
		fetch('/api/beans'),
		fetch('/api/label-brands').catch(() => null)
	]);
	if (!optionsRes.ok) error(500, 'Failed to load the label choices');
	const roasts = roastsRes.ok ? ((await roastsRes.json()) as RecentRoast[]) : [];
	const beans = beansRes.ok
		? ((await beansRes.json()) as {
				id: string;
				name: string;
				label_design?: string | null;
				label_params?: Record<string, unknown> | null;
				label_brand?: string | null;
			}[])
		: [];
	const brands = brandsRes?.ok ? ((await brandsRes.json()) as Brand[]) : [];
	return {
		options: (await optionsRes.json()) as LabelOptions,
		roasts: roasts.filter((r) => r.status === 'completed'),
		beans,
		brands,
		asked: url.searchParams.getAll('roast')
	};
};
