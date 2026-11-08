import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

// The roast's printable page became its report on Reports (docs/reports.md): a PDF the
// server makes. Old links land there.
export const load: PageLoad = ({ params }) => {
	redirect(308, `/reports?kind=roast&roast=${encodeURIComponent(params.id)}`);
};
