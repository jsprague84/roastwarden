import { describe, expect, it } from 'vitest';
import type { PurchaseOrderLine } from '#lib/api/orders.js';
import { fileSize, isOpen, lineSummary, orderName, receivedLines, totals } from './orders.js';

const line = (p: Partial<PurchaseOrderLine>): PurchaseOrderLine => ({
	id: 'l1',
	position: 1,
	kind: 'coffee',
	name: 'Kenya',
	sku: null,
	variant_id: null,
	bag: '1 LB',
	quantity: 1,
	weight_g: 453.6,
	price: 9.15,
	taxed: null,
	url: null,
	bean_id: 'b1',
	bean_name: 'Kenya',
	wanted_id: null,
	move_id: null,
	received_g: null,
	...p
});

describe('orders', () => {
	it('names and states', () => {
		expect(orderName({ supplier: "Sweet Maria's", order_number: '000412345' })).toBe(
			"Sweet Maria's 000412345"
		);
		expect(isOpen({ status: 'shipped' })).toBe(true);
		expect(isOpen({ status: 'received' })).toBe(false);
	});

	it('checks the totals to the cent', () => {
		const lines = [
			line({}),
			line({ id: 'l2', position: 2, price: 9.5 }),
			line({ id: 'l3', position: 3, kind: 'other', price: 3 })
		];
		expect(totals({ lines, discount: 0, shipping: 8.99, tax: 0.21, total: 30.85 })).toEqual({
			lines: 21.65,
			computed: 30.85,
			difference: 0,
			addsUp: true
		});
		expect(totals({ lines, discount: 1, shipping: 8.99, tax: 0.21, total: 30.85 }).difference).toBe(
			1
		);
		expect(lineSummary(lines)).toBe('2 coffees, 1 other');
	});

	it('sends only the weights that changed', () => {
		const lines = [
			line({}),
			line({ id: 'l2', position: 2 }),
			line({ id: 'l3', position: 3, kind: 'other', weight_g: null })
		];
		expect(receivedLines(lines, { l1: 453.6, l2: 440, l3: 5 })).toEqual([
			{ line: '2', weight_g: 440 }
		]);
		expect(receivedLines(lines, {})).toEqual([]);
	});

	it('sizes files', () => {
		expect(fileSize(500)).toBe('500 B');
		expect(fileSize(12_000)).toBe('12 KB');
		expect(fileSize(3_400_000)).toBe('3.2 MB');
	});
});
