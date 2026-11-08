/** Roasted coffee: resting and ready (docs/roasted.md). Lines for lots and purposes, and the move form's amount. */
import type {
	CombinedRef,
	RoastedFor,
	RoastedGroup,
	RoastedLot,
	RoastedMove,
	RoastedMoveRequest
} from '#lib/api/roasted.js';
import type { WeightUnit } from '#lib/stores/units.svelte.js';
import { fmtGrams } from './stock.js';
import { parseWeight, smallUnit } from './weight.js';

/** "6 Nov" from an owner's date, YYYY-MM-DD. */
export function fmtDay(d: string | null | undefined): string {
	if (!d) return '–';
	const [y, m, day] = d.split('-').map(Number);
	if (!y || !m || !day) return d;
	return new Date(Date.UTC(y, m - 1, day, 12)).toLocaleDateString('en-GB', {
		day: 'numeric',
		month: 'short',
		timeZone: 'UTC'
	});
}

/** "Espresso", "Filter", "Either". */
export function forLabel(f: RoastedFor | string): string {
	return f === 'espresso' ? 'Espresso' : f === 'filter' ? 'Filter' : 'Either';
}

/** Where a lot is in its rest: "Resting · ready 6 Nov (in 8 d)", "Ready · day 12 · best until 1 Dec". */
export function stateLine(
	l: Pick<RoastedLot, 'state' | 'ready_from' | 'ready_in_days' | 'age_days' | 'best_until'>
): string {
	switch (l.state) {
		case 'resting':
			return `Resting · ready ${fmtDay(l.ready_from)}${l.ready_in_days != null ? ` (in ${l.ready_in_days} d)` : ''}`;
		case 'ready':
			return `Ready · day ${l.age_days} · best until ${fmtDay(l.best_until)}`;
		case 'past_best':
			return `Past its best (since ${fmtDay(l.best_until)})`;
		case 'unweighed':
			return 'Not weighed: record its roasted weight';
		case 'finished':
			return 'Finished';
	}
}

/** Text colour for a lot's state (Chart paper tokens; amber as the green stock's low). */
export function stateClass(state: RoastedLot['state']): string {
	return state === 'ready'
		? 'text-go'
		: state === 'past_best' || state === 'unweighed'
			? 'text-amber-400'
			: 'text-muted-foreground';
}

/** "#12 Kenya C" */
export function lotName(l: Pick<RoastedLot, 'batch_number' | 'name'>): string {
	return l.batch_number != null ? `#${l.batch_number} ${l.name}` : l.name;
}

/** "Espresso: 260 g ready · 172 g resting, next ready 6 Nov · ~18 g/day, roast by 13 Nov". */
export function groupLine(g: RoastedGroup): string {
	const ready = g.ready_g > 0 ? `${fmtGrams(g.ready_g)} ready` : 'nothing ready';
	let t = `${forLabel(g.for)}: ${ready}${g.resting_g > 0 ? ` · ${fmtGrams(g.resting_g)} resting` : ''}`;
	if (g.next_ready) t += `, next ready ${fmtDay(g.next_ready)}`;
	if (g.past_best_g > 0) t += ` · ${fmtGrams(g.past_best_g)} past its best`;
	if (g.per_day_g != null) {
		t += ` · ~${fmtGrams(g.per_day_g)}/day`;
		if (g.roast_by) t += `, roast by ${fmtDay(g.roast_by)}`;
	}
	if (g.unweighed) t += ` · ${g.unweighed} not weighed`;
	return t;
}

export type MoveAmount = Pick<RoastedMoveRequest, 'grams' | 'doses' | 'bags' | 'all'>;

/** The move form's placeholder: "36 g, 2 doses" or "1.3 oz, 2 doses". */
export function amountHint(weights: WeightUnit = 'metric'): string {
	return weights === 'us' ? '1.3 oz, 2 doses' : '36 g, 2 doses';
}

/**
 * The amount typed in the move form: "36", "36g", "1.3 oz", "2 doses" (shots, cups, brews),
 * "1 bag", "all". A bare number is the browser's small unit (g, or oz in US weights).
 * An error message when it is not one of them.
 */
export function parseAmount(text: string, weights: WeightUnit = 'metric'): MoveAmount | string {
	const t = text.trim().toLowerCase();
	const eg = `${amountHint(weights)}, 1 bag or all`;
	if (!t) return `Say how much: ${eg}`;
	if (['all', 'everything', 'rest', 'the rest'].includes(t)) return { all: true };
	const m = /^(\d+(?:[.,]\d+)?)\s*([a-z]*)$/.exec(t);
	if (!m) return `Not an amount: "${text}" (${eg})`;
	const n = Number(m[1].replace(',', '.'));
	if (!(n > 0)) return 'An amount is more than 0';
	const unit = m[2];
	if (['dose', 'doses', 'shot', 'shots', 'cup', 'cups', 'brew', 'brews'].includes(unit))
		return { doses: n };
	if (['bag', 'bags'].includes(unit)) return { bags: n };
	const g = parseWeight(t, weights);
	if (typeof g === 'number') return { grams: g };
	return `Unknown unit "${unit}" (${smallUnit(weights)}, doses, bags or all)`;
}

/** Grinds offered for a ground bag; the owner may type their own word. */
export const GRINDS = ['espresso', 'filter', 'moka', 'French press', 'Turkish'];

/**
 * Bags typed in the Packed form: "3x250g", "3 × 250 g", "2x8oz", or "3" (the bag size in
 * Settings → Roasting). An error message when it is not.
 */
export function parseBags(
	text: string,
	unit: WeightUnit = 'metric'
): { bags: number; bag_g?: number } | string {
	const t = text.trim().toLowerCase().replace(/[×*]/g, 'x');
	if (!t) return 'Say the bags: 3x250g, or 3 for the bag size';
	const m = /^(\d+)\s*(?:bags?)?\s*(?:x\s*(\d+(?:[.,]\d+)?)\s*(g|gr|grams?|oz|ounces?)?)?$/.exec(t);
	if (!m) return `Not bags: "${text}" (3x250g, or 3 for the bag size)`;
	const bags = Number(m[1]);
	if (!(bags >= 1)) return 'At least one bag';
	if (m[2] == null) return { bags };
	const n = Number(m[2].replace(',', '.'));
	if (!(n > 0)) return 'A bag holds more than 0 g';
	// a bare size is the browser's small unit (g, or oz in US)
	const oz = m[3] ? m[3].startsWith('o') : unit === 'us';
	return { bags, bag_g: oz ? Math.round(n * 28.349523125 * 10) / 10 : n };
}

/** A packed move in words: "2 × 100 g ground for espresso, best until 24 Nov". */
export function packedLine(
	m: Pick<RoastedMove, 'bags' | 'bag_g' | 'form' | 'grind' | 'best_until'>
): string {
	const form = m.form === 'ground' ? `ground${m.grind ? ` for ${m.grind}` : ''}` : 'whole bean';
	const size = m.bag_g != null ? fmtGrams(m.bag_g) : 'bags';
	return `${m.bags ?? '?'} × ${size} ${form}${m.best_until ? `, best until ${fmtDay(m.best_until)}` : ''}`;
}

/** A row of the list: one roast, or a bag of several roasts with them in it (oldest first). */
export type LotRow =
	{ kind: 'lot'; lot: RoastedLot } | { kind: 'bag'; bag: CombinedRef; members: RoastedLot[] };

/** The list's rows: a bag's roasts gathered under it, where its first one comes. */
export function lotRows(lots: RoastedLot[]): LotRow[] {
	const rows: LotRow[] = [];
	const bags = new Map<string, { kind: 'bag'; bag: CombinedRef; members: RoastedLot[] }>();
	for (const l of lots) {
		const c = l.combined;
		if (!c) {
			rows.push({ kind: 'lot', lot: l });
			continue;
		}
		let row = bags.get(c.id);
		if (!row) {
			row = { kind: 'bag', bag: c, members: [] };
			bags.set(c.id, row);
			rows.push(row);
		}
		row.members.push(l);
	}
	for (const r of bags.values())
		r.members.sort((a, b) =>
			a.roasted_at < b.roasted_at ? -1 : a.roasted_at > b.roasted_at ? 1 : 0
		);
	return rows;
}

/** "6–7 Nov", "30 Oct–2 Nov" or "6 Nov": the days a bag's roasts were roasted. */
export function bagDays(b: Pick<CombinedRef, 'first_roasted_on' | 'last_roasted_on'>): string {
	const first = fmtDay(b.first_roasted_on);
	const last = fmtDay(b.last_roasted_on);
	if (first === last) return first;
	const [d1, m1] = first.split(' ');
	const [, m2] = last.split(' ');
	return m1 === m2 ? `${d1}–${last}` : `${first}–${last}`;
}

/** Where a bag is in its rest, as a lot's line: ready when its newest roast is. */
export function bagStateLine(b: CombinedRef): string {
	switch (b.state) {
		case 'resting':
			return `Resting · ready ${fmtDay(b.ready_from)}${b.ready_in_days != null ? ` (in ${b.ready_in_days} d)` : ''}`;
		case 'ready':
			return `Ready · roasted ${bagDays(b)} · best until ${fmtDay(b.best_until)}`;
		case 'past_best':
			return `Past its best (since ${fmtDay(b.best_until)})`;
		default:
			return 'Finished';
	}
}

/** The roasts that can go into one bag with `l` (or into the bag `l` is in): for the same use,
 *  weighed, with coffee left, their coffee recorded and in no bag yet. The same coffee first,
 *  then other coffees (a blend). */
export function combinable(l: RoastedLot, lots: RoastedLot[]): RoastedLot[] {
	if (!l.bean_id) return [];
	const ok = lots.filter(
		(o) =>
			o.session_id !== l.session_id &&
			!!o.bean_id &&
			o.for === l.for &&
			o.state !== 'unweighed' &&
			o.state !== 'finished' &&
			(o.left_g ?? 0) > 0.5 &&
			!o.combined
	);
	return [
		...ok.filter((o) => o.bean_id === l.bean_id),
		...ok.filter((o) => o.bean_id !== l.bean_id)
	];
}

/** "60 % Guji · 40 % Kenya Nyeri": a blend's coffees and their shares; '' for one coffee. */
export function blendLine(b: Pick<CombinedRef, 'blend' | 'coffees'>): string {
	if (!b.blend) return '';
	return b.coffees.map((c) => `${c.share_pct} % ${c.name}`).join(' · ');
}

/** Roasted coffee left for a purpose, as one short line for the history page. */
export function lotSummary(l: RoastedLot): string {
	if (l.state === 'unweighed') return 'Roasted coffee: not weighed yet';
	if (l.combined) {
		const b = l.combined;
		if (b.state === 'finished') return `Roasted coffee: finished, in the bag #${b.batches}`;
		return `Roasted coffee: in one bag with #${b.batches} · ${fmtGrams(b.left_g)} left of ${fmtGrams(b.roasted_g)} · ${bagStateLine(b).replace(/^\w/, (ch) => ch.toLowerCase())}`;
	}
	const left = `${fmtGrams(l.left_g)} left of ${fmtGrams(l.roasted_g)}`;
	if (l.state === 'finished') return `Roasted coffee: finished (${fmtGrams(l.roasted_g)})`;
	return `Roasted coffee: ${left} · ${stateLine(l).replace(/^\w/, (c) => c.toLowerCase())}`;
}
