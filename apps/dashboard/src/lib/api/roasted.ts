/**
 * Roasted coffee (docs/roasted.md): each roast's lot and its moves, roasts in one bag, the stock
 * by purpose.
 */

import { request, tzMinutes } from '#lib/api/http.js';

type RoastedState = 'resting' | 'ready' | 'past_best' | 'finished' | 'unweighed';

/** espresso, filter, or any (omni, other or no purpose: for either) */
export type RoastedFor = 'espresso' | 'filter' | 'any';

export interface RoastedMove {
	id: string;
	session_id: string;
	at: string;
	/** Below 0 takes coffee away */
	delta_g: number;
	kind: 'used' | 'given' | 'packed' | 'discarded' | 'adjust';
	recipient?: string | null;
	note?: string | null;
	/** Packed: bags, grams a bag, whole | ground, the grind, the label run that took them */
	bags?: number | null;
	bag_g?: number | null;
	form?: 'whole' | 'ground' | null;
	grind?: string | null;
	label_run?: string | null;
	/** Packed: the bags' best-until (ground keeps its best for less) */
	best_until?: string | null;
	/** A move on a bag of several roasts: the id its share on each roast carries */
	combine_move?: string | null;
}

/** Roasts of one coffee in one bag or jar (docs/roasted.md, "Roasts in one bag"). */
/** A coffee in a combined lot and its share of the bag */
interface LotCoffee {
	bean_id: string;
	name: string;
	/** Whole percent of what is left (of the roasted weight once nothing is); they add up to 100 */
	share_pct: number;
}

export interface CombinedRef {
	id: string;
	/** Its name, or the coffee's with the batches ("Kenya Nyeri #16+17"); a blend's, its coffees' */
	name: string;
	/** Roasts of more than one coffee */
	blend: boolean;
	/** Its coffees, the largest share first */
	coffees: LotCoffee[];
	/** "16+17", oldest roast first */
	batches: string;
	/** Its roasts' ids, oldest first */
	roasts: string[];
	roasted_g: number;
	left_g: number;
	state: RoastedState;
	/** Ready when its newest roast is */
	ready_from: string;
	ready_in_days?: number | null;
	/** At its best until its oldest roast's best-until */
	best_until: string;
	first_roasted_on: string;
	last_roasted_on: string;
}

interface CombinedMove {
	id: string;
	at: string;
	kind: RoastedMove['kind'];
	delta_g: number;
	recipient?: string | null;
	note?: string | null;
	bags?: number | null;
	bag_g?: number | null;
	form?: 'whole' | 'ground' | null;
	grind?: string | null;
	label_run?: string | null;
	best_until?: string | null;
	split: { session_id: string; batch_number?: number | null; delta_g: number; move_id: string }[];
}

export interface CombinedLot extends CombinedRef {
	note?: string | null;
	created_at: string;
	bean_id?: string | null;
	bean_name?: string | null;
	for: RoastedFor;
	used_g: number;
	given_g: number;
	packed_g: number;
	members: RoastedLot[];
	moves: CombinedMove[];
}

export interface RoastedLot {
	session_id: string;
	name: string;
	device_id: string;
	batch_number?: number | null;
	bean_id?: string | null;
	bean_name?: string | null;
	purpose?: string | null;
	level?: string | null;
	for: RoastedFor;
	roasted_at: string;
	/** YYYY-MM-DD, the owner's date */
	roasted_on: string;
	age_days: number;
	roasted_g?: number | null;
	left_g?: number | null;
	used_g: number;
	given_g: number;
	/** Packed into bags */
	packed_g: number;
	state: RoastedState;
	ready_from: string;
	ready_in_days?: number | null;
	best_until: string;
	rest_days: number;
	/** With one roast only */
	moves?: RoastedMove[];
	/** In one bag with other roasts: its moves are split across them */
	combined?: CombinedRef | null;
}

export interface RoastedGroup {
	for: RoastedFor;
	ready_g: number;
	resting_g: number;
	past_best_g: number;
	left_g: number;
	lots: number;
	unweighed: number;
	next_ready?: string | null;
	next_ready_g?: number | null;
	per_day_g?: number | null;
	lasts_days?: number | null;
	roast_by?: string | null;
	low: boolean;
}

export interface RoastedStock {
	today: string;
	lots: RoastedLot[];
	groups: RoastedGroup[];
	rest_days: { espresso_light: number; espresso: number; filter_light: number; filter: number };
	best_until_days_light: number;
	best_until_days: number;
	/** Ground coffee, days from packing */
	best_until_days_ground: number;
	low_g: number;
}

/** Used, given or thrown away (one of grams, doses, bags, all), packed (bags of bag_g, whole
 *  or ground with a grind), or adjust with set_g. */
export interface RoastedMoveRequest {
	kind: 'used' | 'given' | 'packed' | 'discarded' | 'adjust';
	grams?: number;
	doses?: number;
	bags?: number;
	bag_g?: number;
	form?: 'whole' | 'ground';
	grind?: string | null;
	all?: boolean;
	set_g?: number;
	recipient?: string | null;
	note?: string | null;
	at?: string | null;
}

export const roasted = {
	/** Coffee left per roast and per purpose; `all` lists finished roasts too. */
	stock: (all = false) =>
		request<RoastedStock>(`/api/roasted?tz=${tzMinutes()}${all ? '&all=true' : ''}`),
	/** One roast's coffee with its moves; null when it is not a finished roast (404). */
	lot: async (sessionId: string): Promise<RoastedLot | null> => {
		try {
			return await request<RoastedLot>(
				`/api/roasted/${encodeURIComponent(sessionId)}?tz=${tzMinutes()}`
			);
		} catch (e) {
			if (e instanceof Error && /\b404\b/.test(e.message)) return null;
			throw e;
		}
	},
	move: (sessionId: string, req: RoastedMoveRequest) =>
		request<RoastedLot>(`/api/roasted/${encodeURIComponent(sessionId)}/moves?tz=${tzMinutes()}`, {
			method: 'POST',
			body: JSON.stringify(req)
		}),
	undoMove: (sessionId: string, moveId: string) =>
		request<RoastedLot>(
			`/api/roasted/${encodeURIComponent(sessionId)}/moves/${encodeURIComponent(moveId)}?tz=${tzMinutes()}`,
			{ method: 'DELETE' }
		),
	/** Roasts of one coffee into one bag; with a roast already in a bag, the others join it. */
	combine: (roasts: string[], name?: string | null, note?: string | null) =>
		request<CombinedLot>(`/api/roasted/combined?tz=${tzMinutes()}`, {
			method: 'POST',
			body: JSON.stringify({ roasts, name: name || null, note: note || null })
		}),
	/** A bag of several roasts, by its id or one of its roasts'. */
	combined: (id: string) =>
		request<CombinedLot>(`/api/roasted/combined/${encodeURIComponent(id)}?tz=${tzMinutes()}`),
	/** Split a bag apart: its roasts are separate again, each keeping its share of the moves. */
	uncombine: (id: string) =>
		request<{ roasts: string[] }>(`/api/roasted/combined/${encodeURIComponent(id)}`, {
			method: 'DELETE'
		})
};
