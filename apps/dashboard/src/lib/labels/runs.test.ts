import { describe, expect, it } from 'vitest';
import {
	cellOwners,
	labelCount,
	messageField,
	newItem,
	nextAfter,
	packOffer,
	roastMatches,
	runBody,
	runPlace,
	sheetsFor
} from './runs.js';

describe('print runs', () => {
	const items = [
		{
			roast: 'a',
			name: 'Guji (#7)',
			count: 2,
			design: 'gift',
			message: ' for Sam ',
			weight_g: 250
		},
		{ roast: 'b', name: 'Palmichal (#5)', count: 1, design: '', message: '', weight_g: null }
	];

	it('makes the request: copies at least 1, empty choices left to the server', () => {
		const b = runBody(items, { sheet: '5163', start: 4, note: '  gift baskets ' }, -300);
		expect(b).toEqual({
			sheet: '5163',
			start: 4,
			tz: -300,
			note: 'gift baskets',
			printer: undefined,
			items: [
				{ roast: 'a', count: 2, design: 'gift', message: 'for Sam', weight_g: 250 },
				{ roast: 'b', count: 1, design: undefined, message: undefined, weight_g: undefined }
			]
		});
		expect(labelCount(items)).toBe(3);
		expect(labelCount([{ count: 0 }])).toBe(1);
	});

	it("gives every roast the run's QR choice", () => {
		const b = runBody(items, { sheet: '5163', start: 1, qr: 'share' }, 0);
		expect(b.items.map((i) => i.qr)).toEqual(['share', 'share']);
		expect(runBody(items, { sheet: '5163', start: 1, qr: '' }, 0).items[0].qr).toBeUndefined();
	});

	it('knows which roast each label of the first sheet is', () => {
		expect(cellOwners(items, 3, 10)).toEqual([-1, -1, 0, 0, 1, null, null, null, null, null]);
		expect(cellOwners(items, 9, 10)).toEqual([-1, -1, -1, -1, -1, -1, -1, -1, 0, 0]);
	});

	it('counts sheets and the next free label, a full sheet meaning a new one', () => {
		expect(sheetsFor(3, 9, 10)).toBe(2);
		expect(sheetsFor(3, 1, 10)).toBe(1);
		expect(nextAfter(1, 3, 10)).toBe(4);
		expect(nextAfter(4, 7, 10)).toBe(1);
		expect(nextAfter(9, 5, 10)).toBe(4);
	});

	it("finds a roast by its name, batch or coffee's name", () => {
		expect(roastMatches('', '#5 Test roast')).toBe(true);
		expect(roastMatches('guji', '#48 Light espresso', 'Ethiopia Guji')).toBe(true);
		expect(roastMatches('guji #48', '#48 Light espresso', 'Ethiopia Guji')).toBe(true);
		expect(roastMatches('ESPRESSO', '#48 Light espresso', null)).toBe(true);
		expect(roastMatches('kenya', '#48 Light espresso', 'Ethiopia Guji')).toBe(false);
	});

	it("sends an item's settings, brand, notes and hidden fields only when set", () => {
		const it = newItem('a', 'Guji (#7)', {
			params: { art: 'cup' },
			brand: 'home',
			notes: ' cocoa ',
			hide: ['batch']
		});
		expect(runBody([it], { sheet: '5163', start: 1 }, 0).items[0]).toMatchObject({
			roast: 'a',
			count: 1,
			params: { art: 'cup' },
			brand: 'home',
			notes: 'cocoa',
			hide: ['batch']
		});
		const bare = runBody([newItem('b', 'Palmichal')], { sheet: '5163', start: 1 }, 0).items[0];
		expect([bare.params, bare.brand, bare.notes, bare.hide]).toEqual([
			undefined,
			undefined,
			undefined,
			undefined
		]);
	});

	it('sends ground with its grind and the bags to take off only when asked', () => {
		const g = runBody(
			[newItem('a', 'Guji', { ground: true, grind: ' espresso ', pack: true })],
			{ sheet: '5163', start: 1 },
			0
		).items[0];
		expect([g.form, g.grind, g.pack]).toEqual(['ground', 'espresso', true]);
		const w = runBody([newItem('a', 'Guji', { grind: 'filter' })], { sheet: '5163', start: 1 }, 0)
			.items[0];
		expect([w.form, w.grind, w.pack]).toEqual([undefined, undefined, undefined]);
	});

	it('offers to take the bags off when that much is left', () => {
		expect(packOffer(3, 250, 900, 800)).toEqual({
			bags: 3,
			bagG: 250,
			text: 'Take 3 × 250 g off this roast',
			enough: true
		});
		expect(packOffer(4, 250, 900, 800)?.enough).toBe(false);
		expect(packOffer(1, null, 194, 194, 'the bag #16+17')?.text).toBe(
			'Take 1 × 194 g off the bag #16+17'
		);
		// No weight asked: the roasted weight, one bag of all of it
		expect(packOffer(1, null, 86, 86)).toMatchObject({ bagG: 86, enough: true });
		expect(packOffer(1, null, null, null)).toBeNull();
		expect(packOffer(1, 100, 500, null)?.enough).toBe(false);
	});

	it("names the message as the design uses it, never two To's", () => {
		expect(messageField({ params: {} })).toMatchObject({ show: true, label: 'Message (a gift)' });
		expect(messageField(null).show).toBe(true);
		// stamp: a To: line switch; noel-tag: a To / From block
		expect(
			messageField({ description: 'a To: line (the message)', params: { to: { default: true } } })
		).toMatchObject({ show: true, label: 'To', placeholder: 'Sam' });
		expect(messageField({ params: { from: { default: '' } } }).label).toBe('To');
		// Its own text field for the recipient: no Message as well
		expect(messageField({ params: { to: { default: 'Sam' } } }).show).toBe(false);
		expect(messageField({ message: 'Dedication', params: {} }).label).toBe('Dedication');
		expect(messageField({ message: false }).show).toBe(false);
	});

	it('says where a run is on its sheet', () => {
		expect(runPlace({ start: 1, labels: 3, sheet: '5163' }, 10)).toBe('labels 1–3 of 5163');
		expect(runPlace({ start: 4, labels: 1, sheet: '5163' }, 10)).toBe('label 4 of 5163');
		expect(runPlace({ start: 9, labels: 5, sheet: '5163' }, 10)).toBe(
			'5 labels from label 9 of 5163'
		);
	});
});
