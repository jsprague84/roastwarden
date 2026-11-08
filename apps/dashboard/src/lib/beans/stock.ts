/** Beans and planned roasts: formatting and stock arithmetic (docs/batches.md). */
import type { Bean } from '#lib/api/beans.js';
import type { PlannedRoast } from '#lib/api/plans.js';
import { weightState } from '#lib/stores/units.svelte.js';
import { formatWeight } from './weight.js';

/**
 * A weight in this browser's system (Settings → This browser): "850 g", "1.25 kg" (from 1 kg),
 * "−150 g"; in US "8.8 oz", "2.21 lb" (from 1 lb). `precise` for a roast's weights on a
 * scale ("85.75 g"). `lib/beans/weight.ts` formats it.
 */
export function fmtGrams(g: number | null | undefined, opts?: { precise?: boolean }): string {
	return formatWeight(g, weightState.unit, opts);
}

/** "+2 kg" for a ledger line. */
export function fmtDelta(g: number): string {
	return g > 0 ? `+${fmtGrams(g)}` : fmtGrams(g);
}

export type StockLevel = 'ok' | 'low' | 'short' | 'out';

export interface StockNote {
	level: StockLevel;
	text: string;
}

/**
 * What a roast of `weightG` from this bean leaves, for the set-up step: enough, low
 * afterwards (below the bean's threshold), or not enough for the batch.
 */
export function stockNote(
	bean: Pick<Bean, 'stock_g' | 'low_stock_g'>,
	weightG: number | null | undefined
): StockNote {
	const stock = bean.stock_g;
	const w = weightG != null && Number.isFinite(weightG) && weightG > 0 ? weightG : null;
	if (stock <= 0) return { level: 'out', text: 'Out of stock' };
	if (w != null && w > stock + 1e-6)
		return { level: 'short', text: `Not enough: ${fmtGrams(stock)} in stock for ${fmtGrams(w)}` };
	const after = w != null ? stock - w : stock;
	const low = after < bean.low_stock_g;
	const text =
		w != null
			? `${fmtGrams(stock)} in stock, ${fmtGrams(after)} after this roast`
			: `${fmtGrams(stock)} in stock`;
	return { level: low ? 'low' : 'ok', text: low ? `${text} (low)` : text };
}

/** Text colour for a note; amber for low, red when the batch does not fit. */
export function levelClass(level: StockLevel): string {
	return level === 'ok'
		? 'text-muted-foreground'
		: level === 'low'
			? 'text-amber-400'
			: 'text-red-400';
}

/** One line for a planned roast: "Brazil Cerrado · 100 g · Light espresso". */
export function planLabel(
	p: Pick<PlannedRoast, 'name' | 'bean_name' | 'green_weight_g' | 'profile_name'>,
	{ profile = true }: { profile?: boolean } = {}
): string {
	const parts = [
		p.bean_name,
		p.green_weight_g != null ? fmtGrams(p.green_weight_g) : null,
		profile ? p.profile_name : null
	].filter(Boolean);
	const body = parts.join(' · ');
	if (p.name && body) return `${p.name}: ${body}`;
	return p.name || body || 'Unspecified roast';
}

/**
 * Walk the queue in order and say, per plan, what its bean has left after it, counting
 * the plans before it on the same bean. `short` when that goes below zero.
 */
export function queueStock(plans: PlannedRoast[]): Map<string, { leftG: number; short: boolean }> {
	const left = new Map<string, number>();
	const out = new Map<string, { leftG: number; short: boolean }>();
	for (const p of plans) {
		if (p.session_id || !p.bean_id) continue;
		const before = left.get(p.bean_id) ?? p.bean_stock_g ?? 0;
		const after = before - (p.green_weight_g ?? 0);
		left.set(p.bean_id, after);
		out.set(p.id, { leftG: after, short: after < -1e-6 });
	}
	return out;
}

/** The next plan to offer: the first still planned that was not skipped. */
export function nextPlan(
	plans: PlannedRoast[],
	skipped: ReadonlySet<string>
): { plan: PlannedRoast; index: number; count: number } | null {
	const open = plans.filter((p) => !p.session_id);
	const i = open.findIndex((p) => !skipped.has(p.id));
	return i < 0 ? null : { plan: open[i], index: i, count: open.length };
}

/** "#12", or '' with no number. */
export function batchLabel(n: number | null | undefined): string {
	return n != null && n > 0 ? `#${n}` : '';
}

/** Move one id up or down in an ordered list of ids. */
export function moveId(ids: string[], id: string, by: -1 | 1): string[] {
	const i = ids.indexOf(id);
	const j = i + by;
	if (i < 0 || j < 0 || j >= ids.length) return ids;
	const out = [...ids];
	[out[i], out[j]] = [out[j], out[i]];
	return out;
}
