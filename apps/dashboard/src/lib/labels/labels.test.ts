import { describe, expect, it } from 'vitest';
import { labelQuery, sheetsNeeded, slotBoxes, type LabelSheet } from './labels.js';

const s5163: LabelSheet = {
	id: '5163',
	name: 'Avery 5163',
	page_w: 8.5,
	page_h: 11,
	w: 4,
	h: 2,
	round: false,
	cols: 2,
	rows: 5,
	left: 0.1625,
	top: 0.5,
	pitch_x: 4.1875,
	pitch_y: 2,
	offset: { x_mm: 0, y_mm: 0 }
};

describe('labels', () => {
	it('asks for what was chosen, and leaves out what was not', () => {
		const q = new URLSearchParams(
			labelQuery(
				{
					sheet: '5163',
					design: 'gift',
					count: 2,
					start: 3,
					message: ' for Sam ',
					hide: ['qr', 'batch']
				},
				-300
			)
		);
		expect(Object.fromEntries(q)).toEqual({
			sheet: '5163',
			design: 'gift',
			count: '2',
			start: '3',
			tz: '-300',
			message: 'for Sam',
			hide: 'qr,batch'
		});
		expect(
			labelQuery(
				{ sheet: '5163', design: 'plain', count: 0, start: 0, hide: [], weight_g: 250 },
				60
			)
		).toBe('sheet=5163&design=plain&count=1&start=1&tz=60&weight_g=250');
	});

	it('asks for what the QR code opens when chosen', () => {
		const q = new URLSearchParams(
			labelQuery({ sheet: '5163', design: 'gift', count: 1, start: 1, hide: [], qr: 'share' }, 0)
		);
		expect(q.get('qr')).toBe('share');
		expect(
			labelQuery({ sheet: '5163', design: 'gift', count: 1, start: 1, hide: [] }, 0)
		).not.toContain('qr=');
	});

	it("carries a design's parameters and a brand", () => {
		const q = new URLSearchParams(
			labelQuery(
				{
					sheet: '5163',
					design: 'ascii',
					count: 1,
					start: 1,
					hide: [],
					params: { art: 'bean' },
					brand: 'home'
				},
				0
			)
		);
		expect(q.get('params')).toBe('{"art":"bean"}');
		expect(q.get('brand')).toBe('home');
		expect(
			labelQuery({ sheet: '5163', design: 'plain', count: 1, start: 1, hide: [], params: {} }, 0)
		).not.toContain('params');
	});

	it('places the labels row by row, as the server prints them', () => {
		const b = slotBoxes(s5163);
		expect(b).toHaveLength(10);
		expect(b[1].left).toBeCloseTo(((0.1625 + 4.1875) / 8.5) * 100);
		expect(b[2].top).toBeCloseTo((2.5 / 11) * 100);
		expect(b[0].width).toBeCloseTo((4 / 8.5) * 100);
	});

	it('counts the sheets a run takes', () => {
		expect(sheetsNeeded(s5163, 2, 1)).toBe(1);
		expect(sheetsNeeded(s5163, 2, 10)).toBe(2);
		expect(sheetsNeeded(s5163, 20, 1)).toBe(2);
		expect(sheetsNeeded(s5163, 21, 1)).toBe(3);
	});
});
