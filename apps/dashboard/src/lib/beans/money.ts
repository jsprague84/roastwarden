/** Money for green coffee: formatting, the cost of a purchase, an order's shipping split (docs/green-coffee.md). */
import type { StockMove } from '#lib/api/beans.js';
import { weightState, type WeightUnit } from '#lib/stores/units.svelte.js';
import { bigUnit, perBigUnit } from './weight.js';

/** "$24.25" or "24.25 USD" when the browser does not know the code. */
export function fmtMoney(
	v: number | null | undefined,
	currency: string | null | undefined
): string {
	if (v == null || !Number.isFinite(v)) return '–';
	const cur = (currency || 'USD').toUpperCase();
	try {
		return new Intl.NumberFormat(undefined, { style: 'currency', currency: cur }).format(v);
	} catch {
		return `${v.toFixed(2)} ${cur}`;
	}
}

/** "$24.25/kg", or per lb in US weights (Settings → This browser): "$11.00/lb". */
export function fmtPerKg(
	v: number | null | undefined,
	currency: string | null | undefined,
	unit: WeightUnit = weightState.unit
): string {
	return v == null ? '–' : `${fmtMoney(perBigUnit(v, unit), currency)}/${bigUnit(unit)}`;
}

/** What a purchase cost in all: price, shipping and fees (null without a price). */
export function purchaseTotal(m: Pick<StockMove, 'price' | 'shipping' | 'fees'>): number | null {
	if (m.price == null) return null;
	return m.price + (m.shipping ?? 0) + (m.fees ?? 0);
}

/** Each line's share of an order's shipping (or fees), by weight, in cents; the server splits the same way. */
export function splitByWeight(
	total: number | null | undefined,
	weights: number[]
): (number | null)[] {
	const sum = weights.reduce((a, w) => a + (w > 0 ? w : 0), 0);
	if (total == null || !Number.isFinite(total) || sum <= 0) return weights.map(() => null);
	return weights.map((w) => (w > 0 ? Math.round(((total * w) / sum) * 100) / 100 : null));
}

/** A number typed in a money field, or null when empty or not a number of 0 or more. */
export function moneyOrNull(v: unknown): number | null {
	if (v == null || v === '') return null;
	const n = typeof v === 'number' ? v : Number(String(v).replace(',', '.'));
	return Number.isFinite(n) && n >= 0 ? n : null;
}

/** The fields of one purchase as typed in the form. */
export interface PurchaseDraft {
	/** Grams (the weight field reads the browser's unit) */
	weight: number | null;
	price: number | string | null;
	shipping: number | string | null;
	fees: number | string | null;
	currency: string;
	supplier: string;
	/** YYYY-MM-DD from a date input; empty: now. */
	date: string;
	crop_year: string;
	arrival: string;
	note: string;
}

export function emptyPurchase(supplier = ''): PurchaseDraft {
	return {
		weight: null,
		price: '',
		shipping: '',
		fees: '',
		currency: '',
		supplier,
		date: '',
		crop_year: '',
		arrival: '',
		note: ''
	};
}

/** A local YYYY-MM-DD for a date input. */
export function localDate(iso: string): string {
	const d = new Date(iso);
	const p = (n: number) => String(n).padStart(2, '0');
	return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** A ledger move as a draft to edit. */
export function draftFromMove(m: StockMove): PurchaseDraft {
	const s = (v: number | null | undefined) => (v == null ? '' : v);
	return {
		weight: m.delta_g,
		price: s(m.price),
		shipping: s(m.shipping),
		fees: s(m.fees),
		currency: m.currency ?? '',
		supplier: m.supplier ?? '',
		date: localDate(m.at),
		crop_year: m.crop_year ?? '',
		arrival: m.arrival ?? '',
		note: m.note ?? ''
	};
}

const text = (v: string) => (v.trim() ? v.trim() : null);

/**
 * The request body for a purchase (or an edit): empty fields are null, which clears on an
 * edit. `at` only when the date was typed (or changed from `keepDate`), so an untouched
 * edit keeps the move's time of day.
 */
export function purchaseBody(d: PurchaseDraft, keepDate?: string) {
	const date = d.date && d.date !== keepDate ? d.date : undefined;
	return {
		delta_g: Number(d.weight),
		price: moneyOrNull(d.price),
		shipping: moneyOrNull(d.shipping),
		fees: moneyOrNull(d.fees),
		currency: text(d.currency)?.toUpperCase() ?? null,
		supplier: text(d.supplier),
		crop_year: text(d.crop_year),
		arrival: text(d.arrival),
		note: text(d.note),
		...(date ? { at: date } : {})
	};
}
