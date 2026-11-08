import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	const [b, p, r, c, w, rs, o] = await Promise.all([
		fetch('/api/beans?archived=true'),
		fetch('/api/plan'),
		fetch('/api/profiles?include_private=true'),
		fetch('/api/costs?months=12'),
		fetch('/api/wanted'),
		// Roasted coffee (docs/roasted.md): dates are the owner's, so the browser's offset
		fetch(`/api/roasted?tz=${-new Date().getTimezoneOffset()}`),
		// Supplier orders not received yet (docs/green-coffee.md, "Orders")
		fetch('/api/orders')
	]);
	if (!b.ok || !p.ok) error(500, 'Failed to load beans');
	return {
		beans: await b.json(),
		plans: await p.json(),
		profiles: r.ok ? await r.json() : [],
		costs: c.ok ? await c.json() : null,
		wanted: w.ok ? await w.json() : [],
		roasted: rs.ok ? await rs.json() : null,
		orders: o.ok ? await o.json() : []
	};
};
