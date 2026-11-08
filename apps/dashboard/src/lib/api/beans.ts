/**
 * Green coffee (docs/batches.md, docs/green-coffee.md): beans and their stock ledger, purchases
 * and costs, a bean's guide, a coffee read from a supplier's link.
 */

import { request } from '#lib/api/http.js';
import type { RoastSession } from '#lib/api/sessions.js';
import type { Draft } from '#lib/api/wanted.js';

/** One line of a bean's guide (docs/new-bean.md): `b1`…`b6` before, `a1`…`a6` after. */
interface GuideLine {
	key: string;
	text: string;
	/** Where it comes from and its grade */
	source: string;
	/** `line`, or `hint` (context, never amber) */
	kind: 'line' | 'hint';
	/** `auto_drop_development`: the profile edit that drops by development */
	action?: 'auto_drop_development' | null;
}

export interface BeanGuide {
	bean_id: string;
	bean: string;
	/** before: no finished roast here; next: after `after`; settled: three or more on the profile */
	stage: 'before' | 'next' | 'settled';
	roaster: string | null;
	profile_id: string | null;
	profile: string | null;
	after: {
		session_id: string;
		name: string;
		batch_number: number | null;
		roasted_at: string;
	} | null;
	reads: {
		process: string;
		process_as_given: string | null;
		peaberry: boolean;
		supplier: string | null;
		level: string | null;
		espresso: string | null;
		density_g_l: number | null;
		moisture_pct: number | null;
	};
	roasts: number;
	roasts_on_profile: number;
	first_crack: {
		bt: number;
		window_c: number;
		t: number | null;
		window_s: number | null;
		basis: string;
	} | null;
	lines: GuideLine[];
	/** After a roast: "If it tastes…" */
	tasting: string | null;
}

export interface Bean {
	id: string;
	name: string;
	origin: string | null;
	variety: string | null;
	process: string | null;
	lot: string | null;
	supplier: string | null;
	notes: string | null;
	moisture_pct: number | null;
	density_g_l: number | null;
	low_stock_g: number;
	archived: boolean;
	created_at: string;
	updated_at: string;
	/** Sum of the ledger, g. */
	stock_g: number;
	roast_count: number;
	/** Stock below low_stock_g. */
	low: boolean;
	/** Ordered or shipped, not received yet (docs/green-coffee.md, "Orders"), g. */
	on_order_g?: number;
	/** Weighted average of the priced purchases in what is left (docs/green-coffee.md). */
	cost_per_kg?: number | null;
	stock_value?: number | null;
	/** The home currency, with a cost. */
	currency?: string | null;
	/** With no label notes: tasting words from the supplier's notes, to start from. */
	label_notes_suggested?: string | null;
	// What the supplier publishes (migration 028)
	region?: string | null;
	producer?: string | null;
	altitude?: string | null;
	grade?: string | null;
	screen_size?: string | null;
	/** Comma-separated. */
	certifications?: string | null;
	drying?: string | null;
	score?: number | null;
	roast_recommendation?: string | null;
	cupping_notes?: string | null;
	url?: string | null;
	sku?: string | null;
	details?: Record<string, string> | null;
	/** A few tasting words its labels print, "chocolate · coconut · apple" (docs/labels.md). */
	label_notes?: string | null;
	/** The name its labels print when its own is long. */
	short_name?: string | null;
}

export interface StockMove {
	id: string;
	bean_id: string;
	at: string;
	delta_g: number;
	kind: 'purchase' | 'adjust' | 'roast';
	session_id: string | null;
	note: string | null;
	price?: number | null;
	shipping?: number | null;
	fees?: number | null;
	/** ISO 4217; null means the home currency. */
	currency?: string | null;
	supplier?: string | null;
	crop_year?: string | null;
	arrival?: string | null;
	order_id?: string | null;
	/** Priced purchase: what it cost. Draw: what the beans were worth then. Home currency. */
	cost?: number | null;
	/** The bean's cost per kg after this move. */
	cost_per_kg?: number | null;
}

/** What a purchase cost and where it came from (money in currency, default the home one). */
export interface PurchaseFields {
	price?: number | null;
	shipping?: number | null;
	fees?: number | null;
	currency?: string | null;
	supplier?: string | null;
	crop_year?: string | null;
	arrival?: string | null;
	/** YYYY-MM-DD or RFC 3339; default now. */
	at?: string | null;
}

interface OrderLine {
	bean_id: string;
	weight_g: number;
	price?: number | null;
	crop_year?: string | null;
	arrival?: string | null;
}

export interface OrderRequest {
	supplier?: string | null;
	currency?: string | null;
	/** Split over the lines by weight. */
	shipping?: number | null;
	fees?: number | null;
	at?: string | null;
	note?: string | null;
	lines: OrderLine[];
}

export interface RoastCost {
	currency: string;
	green_cost: number;
	green_per_kg: number;
	roasted_per_kg: number | null;
	per_espresso_dose: number | null;
	per_filter_dose: number | null;
	per_bag: number | null;
	espresso_dose_g: number;
	filter_dose_g: number;
	bag_g: number;
}

interface MonthCosts {
	/** YYYY-MM (UTC). */
	month: string;
	spent: number;
	spent_other: Record<string, number>;
	bought_g: number;
	roasts: number;
	green_roasted_g: number;
	roasted_g: number;
	roast_cost: number;
	costed_roasts: number;
}

export interface CostReport {
	currency: string;
	months: MonthCosts[];
}

export interface BeanDetail extends Bean {
	moves: StockMove[];
	roasts: RoastSession[];
}

/** Create or edit; on edit absent fields are kept and null clears. */
export interface BeanRequest {
	name?: string;
	origin?: string | null;
	variety?: string | null;
	process?: string | null;
	lot?: string | null;
	supplier?: string | null;
	notes?: string | null;
	moisture_pct?: number | null;
	density_g_l?: number | null;
	low_stock_g?: number;
	archived?: boolean;
	/** Create only: the first purchase, g. */
	stock_g?: number;
	region?: string | null;
	producer?: string | null;
	altitude?: string | null;
	grade?: string | null;
	screen_size?: string | null;
	certifications?: string | null;
	drying?: string | null;
	score?: number | null;
	roast_recommendation?: string | null;
	cupping_notes?: string | null;
	url?: string | null;
	sku?: string | null;
	details?: Record<string, string> | null;
	/** A few tasting words its labels print, "chocolate · coconut · apple" (docs/labels.md). */
	label_notes?: string | null;
	/** The name its labels print when its own is long. */
	short_name?: string | null;
}

/** Suggestions per field: earlier entries first, then seeded lists. */
export interface PickLists {
	supplier: string[];
	origin: string[];
	region: string[];
	variety: string[];
	process: string[];
	drying: string[];
	certifications: string[];
}

export const beans = {
	list: (archived = false) => request<Bean[]>(`/api/beans${archived ? '?archived=true' : ''}`),
	get: (id: string) => request<BeanDetail>(`/api/beans/${encodeURIComponent(id)}`),
	/** Before a bean's first roast, what to expect; after one, what to change (docs/new-bean.md). */
	guide: (
		id: string,
		q: { profile?: string | null; roaster?: string | null; after?: string | null } = {}
	) => {
		const qs = new URLSearchParams(Object.entries(q).filter((e): e is [string, string] => !!e[1]));
		const tail = qs.toString();
		return request<BeanGuide>(
			`/api/beans/${encodeURIComponent(id)}/guide${tail ? `?${tail}` : ''}`
		);
	},
	create: (req: BeanRequest) =>
		request<Bean>('/api/beans', { method: 'POST', body: JSON.stringify(req) }),
	update: (id: string, req: BeanRequest) =>
		request<Bean>(`/api/beans/${encodeURIComponent(id)}`, {
			method: 'PUT',
			body: JSON.stringify(req)
		}),
	/** Deletes a bean nobody roasted; archives one with roasts. */
	delete: (id: string) =>
		request<{ result: 'deleted' | 'archived' }>(`/api/beans/${encodeURIComponent(id)}`, {
			method: 'DELETE'
		}),
	/** A purchase (delta_g, with its price) or "set stock to" (set_g). */
	stock: (
		id: string,
		req: { delta_g?: number; set_g?: number; note?: string | null } & PurchaseFields
	) =>
		request<Bean>(`/api/beans/${encodeURIComponent(id)}/stock`, {
			method: 'POST',
			body: JSON.stringify(req)
		}),
	/** Edit a purchase or correction; absent kept, null clears. */
	updateMove: (
		id: string,
		moveId: string,
		req: { delta_g?: number; note?: string | null } & PurchaseFields
	) =>
		request<StockMove>(`/api/beans/${encodeURIComponent(id)}/moves/${encodeURIComponent(moveId)}`, {
			method: 'PUT',
			body: JSON.stringify(req)
		}),
	deleteMove: (id: string, moveId: string) =>
		request<void>(`/api/beans/${encodeURIComponent(id)}/moves/${encodeURIComponent(moveId)}`, {
			method: 'DELETE'
		}),
	/** One order: a purchase per bean, shipping and fees split by weight. */
	order: (req: OrderRequest) =>
		request<{ order_id: string; moves: StockMove[] }>('/api/purchases', {
			method: 'POST',
			body: JSON.stringify(req)
		}),
	costs: (months = 12) => request<CostReport>(`/api/costs?months=${months}`),
	suggestions: () => request<PickLists>('/api/beans/suggestions'),
	/** Read a coffee from a supplier's link: a preview, nothing is saved. */
	importLink: (url: string) =>
		request<Draft>('/api/beans/import', { method: 'POST', body: JSON.stringify({ url }) }),
	/** null when unknown: no bean, no green weight, no priced purchase before it. */
	roastCost: (sessionId: string) =>
		request<RoastCost | null>(`/api/sessions/${encodeURIComponent(sessionId)}/cost`)
};
