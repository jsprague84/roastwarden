import { describe, expect, it } from 'vitest';
import { judge, isOutside } from './judge.js';
import type { Expectation } from './expect.js';

const exp: Expectation = { range: [14, 16], source: 'default', basis: '', sources: ['mondry2004'] };

describe('judge', () => {
	it('has three states, and waits without a value', () => {
		expect(judge(15, exp)?.state).toBe('within');
		expect(judge(14, exp)?.state).toBe('within');
		expect(judge(13.9, exp)?.state).toBe('below');
		expect(judge(21.7, exp)?.state).toBe('above');
		expect(judge(null, exp)?.state).toBe('waiting');
		expect(judge(NaN, exp)?.state).toBe('waiting');
		expect(judge(15, null)).toBeNull();
		expect(isOutside(judge(21.7, exp))).toBe(true);
		expect(isOutside(judge(null, exp))).toBe(false);
		expect(isOutside(null)).toBe(false);
	});
});
