import { describe, expect, it } from 'vitest';
import { rorEvents, rorEventText } from './ror.js';

/** RoR every second from 0 to `end` s. */
const series = (f: (t: number) => number, end = 700): [number, number][] =>
	Array.from({ length: end + 1 }, (_, t) => [t, f(t)]);

describe('rorEvents', () => {
	it('finds nothing on a smooth declining RoR', () => {
		expect(
			rorEvents(
				series((t) => 20 - t / 50),
				500,
				680
			)
		).toEqual([]);
	});

	it('finds a rise after first crack', () => {
		const ror = series((t) => (t < 560 ? 8 - (t - 500) / 30 : 6 + 3));
		const ev = rorEvents(ror, 500, 680);
		expect(ev.map((e) => e.kind)).toEqual(['rise']);
		expect(ev[0].size).toBeGreaterThanOrEqual(2);
		expect(rorEventText(ev[0])).toMatch(/^RoR rose after first crack/);
	});

	it('ignores a short blip', () => {
		const ror = series((t) => (t >= 560 && t < 570 ? 12 : 8 - (t - 500) / 100));
		expect(rorEvents(ror, 500, 680)).toEqual([]);
	});

	it('finds a crash at first crack', () => {
		const ror = series((t) => (t < 495 ? 14 : t < 510 ? 14 - ((t - 495) * 10) / 15 : 4));
		const ev = rorEvents(ror, 500, 680);
		expect(ev[0]).toMatchObject({ kind: 'crash' });
		expect(ev[0].size).toBeCloseTo(10, 0);
	});

	it('finds a stall', () => {
		const ror = series((t) => (t < 600 ? 5 - (t - 500) / 25 : 0.2));
		const ev = rorEvents(ror, 500, 680);
		expect(ev.find((e) => e.kind === 'stall')?.size).toBeGreaterThanOrEqual(30);
	});

	it('needs a first crack', () => {
		expect(
			rorEvents(
				series(() => 5),
				null,
				600
			)
		).toEqual([]);
	});
});
