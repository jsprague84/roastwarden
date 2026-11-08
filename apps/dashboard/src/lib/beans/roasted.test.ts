import { describe, expect, it } from 'vitest';
import type { CombinedRef, RoastedGroup, RoastedLot } from '#lib/api/roasted.js';
import {
	bagDays,
	bagStateLine,
	blendLine,
	combinable,
	fmtDay,
	groupLine,
	lotName,
	lotRows,
	lotSummary,
	packedLine,
	parseAmount,
	parseBags,
	stateClass,
	stateLine
} from './roasted.js';

const lot = (over: Partial<RoastedLot> = {}): RoastedLot => ({
	session_id: 's',
	name: 'Kenya C',
	device_id: 'esp32_roaster_01',
	batch_number: 12,
	for: 'espresso',
	roasted_at: '2026-10-28T04:03:00Z',
	roasted_on: '2026-10-27',
	age_days: 2,
	roasted_g: 86,
	left_g: 50,
	used_g: 36,
	given_g: 0,
	packed_g: 0,
	state: 'resting',
	ready_from: '2026-11-06',
	ready_in_days: 8,
	best_until: '2026-12-01',
	rest_days: 10,
	...over
});

describe('roasted coffee lines', () => {
	it('dates read as the owner wrote them', () => {
		expect(fmtDay('2026-11-06')).toBe('6 Nov');
		expect(fmtDay(null)).toBe('–');
	});

	it('says where a lot is in its rest', () => {
		expect(stateLine(lot())).toBe('Resting · ready 6 Nov (in 8 d)');
		expect(stateLine(lot({ state: 'ready', age_days: 12, ready_in_days: null }))).toBe(
			'Ready · day 12 · best until 1 Dec'
		);
		expect(stateLine(lot({ state: 'past_best' }))).toBe('Past its best (since 1 Dec)');
		expect(stateLine(lot({ state: 'unweighed' }))).toMatch(/^Not weighed/);
		expect(stateClass('ready')).toBe('text-go');
		expect(stateClass('past_best')).toBe('text-amber-400');
		expect(lotName(lot())).toBe('#12 Kenya C');
		expect(lotName(lot({ batch_number: null }))).toBe('Kenya C');
	});

	it('one line for a roast on its history page', () => {
		expect(lotSummary(lot())).toBe(
			'Roasted coffee: 50 g left of 86 g · resting · ready 6 Nov (in 8 d)'
		);
		expect(lotSummary(lot({ state: 'finished', left_g: 0 }))).toBe(
			'Roasted coffee: finished (86 g)'
		);
	});

	it('a purpose in one line', () => {
		const g: RoastedGroup = {
			for: 'espresso',
			ready_g: 260,
			resting_g: 172,
			past_best_g: 0,
			left_g: 432,
			lots: 5,
			unweighed: 0,
			next_ready: '2026-11-06',
			next_ready_g: 86,
			per_day_g: 18.2,
			lasts_days: 23.7,
			roast_by: '2026-11-13',
			low: false
		};
		expect(groupLine(g)).toBe(
			'Espresso: 260 g ready · 172 g resting, next ready 6 Nov · ~18 g/day, roast by 13 Nov'
		);
		expect(groupLine({ ...g, ready_g: 0, per_day_g: null })).toBe(
			'Espresso: nothing ready · 172 g resting, next ready 6 Nov'
		);
		expect(groupLine({ ...g, resting_g: 0, next_ready: null, per_day_g: null })).toBe(
			'Espresso: 260 g ready'
		);
		expect(
			groupLine({
				...g,
				for: 'any',
				per_day_g: null,
				next_ready: null,
				past_best_g: 40,
				unweighed: 1
			})
		).toBe('Either: 260 g ready · 172 g resting · 40 g past its best · 1 not weighed');
	});
});

describe('the move form reads an amount', () => {
	it('as people type it', () => {
		expect(parseAmount('36')).toEqual({ grams: 36 });
		expect(parseAmount('36 g')).toEqual({ grams: 36 });
		expect(parseAmount('2 doses')).toEqual({ doses: 2 });
		expect(parseAmount('2shots')).toEqual({ doses: 2 });
		expect(parseAmount('1 bag')).toEqual({ bags: 1 });
		expect(parseAmount('All')).toEqual({ all: true });
		expect(parseAmount('4oz')).toEqual({ grams: 113.4 });
		expect(parseAmount('0.5 lb')).toEqual({ grams: 226.8 });
		// US weights: a bare number is ounces
		expect(parseAmount('2', 'us')).toEqual({ grams: 56.7 });
		expect(parseAmount('36 g', 'us')).toEqual({ grams: 36 });
	});

	it('reads packed bags as people type them', () => {
		expect(parseBags('3x250g')).toEqual({ bags: 3, bag_g: 250 });
		expect(parseBags('2 × 100 g')).toEqual({ bags: 2, bag_g: 100 });
		expect(parseBags('3x8', 'us')).toEqual({ bags: 3, bag_g: 226.8 });
		expect(parseBags('3x250', 'metric')).toEqual({ bags: 3, bag_g: 250 });
		expect(parseBags('3')).toEqual({ bags: 3 });
		expect(parseBags('3 bags')).toEqual({ bags: 3 });
		expect(parseBags('1x8oz')).toEqual({ bags: 1, bag_g: 226.8 });
		expect(parseBags('')).toMatch(/Say the bags/);
		expect(parseBags('1.5x250')).toMatch(/Not bags/);
		expect(parseBags('0x250')).toMatch(/At least one/);
		expect(
			packedLine({
				bags: 2,
				bag_g: 100,
				form: 'ground',
				grind: 'espresso',
				best_until: '2026-11-24'
			})
		).toBe('2 × 100 g ground for espresso, best until 24 Nov');
		expect(packedLine({ bags: 3, bag_g: 250, form: 'whole' })).toBe('3 × 250 g whole bean');
	});

	it('or says what is wrong', () => {
		expect(parseAmount('')).toMatch(/Say how much/);
		expect(parseAmount('0')).toMatch(/more than 0/);
		expect(parseAmount('lots')).toMatch(/Not an amount/);
		expect(parseAmount('3 kegs')).toMatch(/Unknown unit/);
	});
});

describe('roasts in one bag', () => {
	const bag: CombinedRef = {
		id: 'bag',
		name: 'Ethiopia Senna Kata #16+17',
		batches: '16+17',
		roasts: ['a', 'b'],
		roasted_g: 194,
		left_g: 137,
		state: 'resting',
		ready_from: '2026-11-14',
		ready_in_days: 4,
		best_until: '2026-12-11',
		first_roasted_on: '2026-11-06',
		last_roasted_on: '2026-11-07',
		blend: false,
		coffees: [{ bean_id: 'eth', name: 'Ethiopia Senna Kata', share_pct: 100 }]
	};
	const a = lot({
		session_id: 'a',
		batch_number: 16,
		bean_id: 'eth',
		for: 'filter',
		roasted_at: '2026-11-07T01:49:00Z',
		combined: bag
	});
	const b = lot({
		session_id: 'b',
		batch_number: 17,
		bean_id: 'eth',
		for: 'filter',
		roasted_at: '2026-11-07T02:10:00Z',
		combined: bag
	});
	const c = lot({ session_id: 'c', batch_number: 18, bean_id: 'eth', for: 'filter' });
	const k = lot({ session_id: 'k', batch_number: 13, bean_id: 'kenya', for: 'filter' });

	it("gathers a bag's roasts under it, where its first one comes", () => {
		const rows = lotRows([c, b, k, a]);
		expect(
			rows.map((r) =>
				r.kind === 'bag' ? `bag:${r.members.map((m) => m.session_id).join(',')}` : r.lot.session_id
			)
		).toEqual(['c', 'bag:a,b', 'k']);
	});

	it("reads a bag's days and rest", () => {
		expect(bagDays(bag)).toBe('6–7 Nov');
		expect(bagDays({ first_roasted_on: '2026-10-30', last_roasted_on: '2026-11-02' })).toBe(
			'30 Oct–2 Nov'
		);
		expect(bagDays({ first_roasted_on: '2026-11-06', last_roasted_on: '2026-11-06' })).toBe(
			'6 Nov'
		);
		expect(bagStateLine(bag)).toBe('Resting · ready 14 Nov (in 4 d)');
		expect(bagStateLine({ ...bag, state: 'ready', ready_in_days: null })).toBe(
			'Ready · roasted 6–7 Nov · best until 11 Dec'
		);
		expect(lotSummary(a)).toBe(
			'Roasted coffee: in one bag with #16+17 · 137 g left of 194 g · resting · ready 14 Nov (in 4 d)'
		);
	});

	it('offers the same use, in no bag yet, to combine: the same coffee first, then a blend', () => {
		const all = [
			k,
			a,
			b,
			c,
			lot({ session_id: 'u', bean_id: 'eth', for: 'filter', state: 'unweighed' })
		];
		expect(combinable(a, all).map((l) => l.session_id)).toEqual(['c', 'k']);
		const d = lot({ session_id: 'd', bean_id: 'eth', for: 'filter' });
		const e = lot({ session_id: 'e', bean_id: 'eth', for: 'espresso' });
		const x = lot({ session_id: 'x', bean_id: null, for: 'filter' });
		expect(combinable(c, [...all, d, e, x]).map((l) => l.session_id)).toEqual(['d', 'k']);
		expect(combinable(lot({ bean_id: null }), all)).toEqual([]);
	});

	it("reads a blend's coffees and shares", () => {
		expect(blendLine(bag)).toBe('');
		expect(
			blendLine({
				blend: true,
				coffees: [
					{ bean_id: 'eth', name: 'Guji', share_pct: 60 },
					{ bean_id: 'kenya', name: 'Kenya Nyeri', share_pct: 40 }
				]
			})
		).toBe('60 % Guji · 40 % Kenya Nyeri');
	});
});
