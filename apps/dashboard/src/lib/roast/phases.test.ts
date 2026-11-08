import { describe, expect, it } from 'vitest';
import { phaseTimes } from './phases.js';

const ev = (event_type: string, elapsed_seconds: number) => ({ event_type, elapsed_seconds });
// Batch #5: CHARGE 7.2 s after Start, dry end 238.7, FC 456.3, DROP 612.6.
const batch5 = [
	ev('charge', 7.2),
	ev('drying_end', 238.7),
	ev('first_crack_start', 456.3),
	ev('drop', 612.6)
];

describe('phaseTimes', () => {
	it('counts from CHARGE, as the server does', () => {
		const p = phaseTimes(batch5, 605);
		expect(p.total).toBeCloseTo(605.4, 1);
		expect(p.drying).toBeCloseTo(231.5, 1);
		expect(p.development).toBeCloseTo(156.3, 1);
		// 25.8 % as the server and the readout say (25.5 % from session time).
		expect(p.dtr).toBe(25.8);
	});

	it('runs to now while roasting', () => {
		const live = batch5.slice(0, 3);
		const p = phaseTimes(live, null, 500);
		expect(p.total).toBe(500);
		expect(p.development).toBeNull();
		expect(p.dtr).toBeCloseTo(((507.2 - 456.3) / 500) * 100, 0);
		expect(phaseTimes([ev('charge', 5)], null, null).total).toBeNull();
	});
});
