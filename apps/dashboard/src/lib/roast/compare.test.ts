import { describe, it, expect } from 'vitest';
import { alignOffset, alignedBt, deltaToReference } from './compare.js';
import type { RoastEvent } from '#lib/api/events.js';

const ev = (event_type: string, elapsed_seconds: number) =>
	({
		id: event_type,
		session_id: 's',
		event_type,
		elapsed_seconds,
		temperature: null,
		notes: null,
		created_at: ''
	}) as RoastEvent;

describe('comparator', () => {
	it('aligns on a landmark, falling back to CHARGE', () => {
		const events = [ev('charge', 12), ev('first_crack_start', 500)];
		expect(alignOffset(events, 'charge')).toEqual({ offset: 12, missing: false });
		expect(alignOffset(events, 'first_crack_start')).toEqual({ offset: 500, missing: false });
		expect(alignOffset(events, 'drop')).toEqual({ offset: 12, missing: true });
		expect(alignOffset([], 'charge')).toEqual({ offset: 0, missing: false });
	});
	it('differences BT against the reference on a common grid', () => {
		const tele = (k: number) =>
			Array.from(
				{ length: 101 },
				(_, i) => ({ elapsed_seconds: i, bean_temp: 100 + k * i }) as never
			);
		const ref = alignedBt(tele(1), 0);
		const other = alignedBt(tele(1.1), 10);
		const d = deltaToReference(ref, other, 0, 100, 10);
		// other at aligned t is 100 + 1.1 (t + 10); ref is 100 + t.
		expect(d[0]).toEqual([0, 11]);
		expect(d.at(-1)![0]).toBe(90);
		expect(d.at(-1)![1]).toBeCloseTo(1.1 * 100 - 90, 6);
	});
});
