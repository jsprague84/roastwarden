/**
 * The shopping list (docs/green-coffee.md): wanted coffees, a draft from a supplier's page, cart
 * links.
 */

import type { Bean, StockMove } from '#lib/api/beans.js';
import { request } from '#lib/api/http.js';

interface DraftSize {
	label: string;
	weight_g: number | null;
	price: number | null;
	currency?: string | null;
	sku?: string | null;
	available?: boolean | null;
	/** The shop's own id for this bag (a Shopify variant): what a cart link takes */
	variant_id?: string | null;
}

/** A coffee read from a supplier's link (nothing saved): a bean's fields plus bag sizes. */
export interface Draft {
	name: string;
	supplier: string | null;
	origin: string | null;
	region: string | null;
	producer: string | null;
	variety: string | null;
	process: string | null;
	drying: string | null;
	altitude: string | null;
	grade: string | null;
	screen_size: string | null;
	certifications: string | null;
	score: number | null;
	roast_recommendation: string | null;
	cupping_notes: string | null;
	lot: string | null;
	arrival: string | null;
	url: string;
	product_id?: string | null;
	sku: string | null;
	details: Record<string, string>;
	sizes: DraftSize[];
}

/** A shopping-list entry. */
export interface Wanted {
	id: string;
	name: string;
	supplier: string | null;
	url: string | null;
	bean_id: string | null;
	size_label: string | null;
	weight_g: number | null;
	price: number | null;
	currency: string | null;
	notes: string | null;
	/** Bags of that size (migration 055). */
	quantity: number;
	draft: Draft | null;
	/** ordered: an order's line fills it until the order is received. */
	status: 'wanted' | 'ordered' | 'bought';
	move_id: string | null;
	order_line_id?: string | null;
	bought_at: string | null;
	created_at: string;
	updated_at: string;
	bean_name: string | null;
}

interface CartLine {
	wanted_id: string;
	name: string;
	size_label: string | null;
	/** Bags (pounds at a store that sells by the pound). */
	quantity: number;
	variant_id: string | null;
	price: number | null;
	seen_price: number | null;
	available: boolean | null;
	checked: boolean;
	in_link: boolean;
	note: string | null;
}

interface Cart {
	supplier: string;
	host: string;
	how: 'permalink' | 'cart' | 'checkout_link';
	url: string | null;
	lines: CartLine[];
	total: number;
	currency: string | null;
	warnings: string[];
}

export interface Carts {
	carts: Cart[];
	left_out: { wanted_id: string; name: string; why: string }[];
}

export const wanted = {
	list: (all = false) => request<Wanted[]>(`/api/wanted${all ? '?all=true' : ''}`),
	add: (req: {
		draft?: Draft;
		bean_id?: string;
		name?: string;
		size?: string;
		weight_g?: number;
		price?: number;
		notes?: string;
	}) => request<Wanted>('/api/wanted', { method: 'POST', body: JSON.stringify(req) }),
	update: (
		id: string,
		req: {
			name?: string;
			size?: string;
			weight_g?: number;
			price?: number;
			notes?: string;
			quantity?: number;
		}
	) =>
		request<Wanted>(`/api/wanted/${encodeURIComponent(id)}`, {
			method: 'PUT',
			body: JSON.stringify(req)
		}),
	delete: (id: string) =>
		request<void>(`/api/wanted/${encodeURIComponent(id)}`, { method: 'DELETE' }),
	/** One link per supplier that opens a filled cart (docs/green-coffee.md, "Cart links"): the server reads each bag again first. */
	carts: (req: { supplier?: string; ids?: string[]; offline?: boolean } = {}) =>
		request<Carts>('/api/wanted/cart', { method: 'POST', body: JSON.stringify(req) }),
	bought: (
		id: string,
		req: {
			weight_g?: number;
			price?: number;
			shipping?: number;
			fees?: number;
			at?: string;
			bean_id?: string;
		}
	) =>
		request<{ wanted: Wanted; bean: Bean; purchase: StockMove | null }>(
			`/api/wanted/${encodeURIComponent(id)}/bought`,
			{
				method: 'POST',
				body: JSON.stringify(req)
			}
		)
};
