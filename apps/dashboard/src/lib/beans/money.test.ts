import { describe, expect, it } from 'vitest';
import {
	emptyPurchase,
	fmtMoney,
	fmtPerKg,
	moneyOrNull,
	purchaseBody,
	purchaseTotal,
	splitByWeight
} from './money.js';

describe('money', () => {
	it('formats a currency, and an unknown code as text', () => {
		expect(fmtMoney(24.25, 'USD')).toMatch(/24\.25/);
		expect(fmtMoney(null, 'USD')).toBe('–');
		expect(fmtMoney(3, 'ZZZ')).toMatch(/3\.00/);
	});

	it('prices per kg, or per lb in US weights', () => {
		expect(fmtPerKg(20, 'USD', 'metric')).toMatch(/20\.00\/kg$/);
		expect(fmtPerKg(20, 'USD', 'us')).toMatch(/9\.07\/lb$/);
		expect(fmtPerKg(null, 'USD', 'us')).toBe('–');
	});

	it('adds shipping and fees to the price', () => {
		expect(purchaseTotal({ price: 46, shipping: 9.5, fees: null })).toBe(55.5);
		expect(purchaseTotal({ price: null, shipping: 9.5, fees: 1 })).toBeNull();
	});

	it('splits an order by weight', () => {
		expect(splitByWeight(12, [1000, 2000])).toEqual([4, 8]);
		expect(splitByWeight(10, [453.6, 0, 907.2])).toEqual([3.33, null, 6.67]);
		expect(splitByWeight(null, [1])).toEqual([null]);
	});

	it('reads money typed with a comma', () => {
		expect(moneyOrNull('9,50')).toBe(9.5);
		expect(moneyOrNull('')).toBeNull();
		expect(moneyOrNull(-1)).toBeNull();
	});

	it('builds a purchase body: empty is null, the date only when typed', () => {
		const d = {
			...emptyPurchase("Sweet Maria's"),
			weight: 2268,
			price: '46',
			currency: 'usd ',
			date: '2026-10-20'
		};
		expect(purchaseBody(d)).toEqual({
			delta_g: 2268,
			price: 46,
			shipping: null,
			fees: null,
			currency: 'USD',
			supplier: "Sweet Maria's",
			crop_year: null,
			arrival: null,
			note: null,
			at: '2026-10-20'
		});
		expect(purchaseBody(d, '2026-10-20')).not.toHaveProperty('at');
	});
});
