import { describe, expect, it } from 'vitest';
import { nextReason } from './prompt.js';

describe('nextReason', () => {
	it('keeps "missing" when a refusal arrives while the prompt is open for it', () => {
		expect(nextReason(true, 'missing', 'rejected')).toBe('missing');
	});

	it('takes "missing" when the sign-in check answers after a refusal opened the prompt', () => {
		expect(nextReason(true, 'rejected', 'missing')).toBe('missing');
	});

	it('takes the reason asked when the prompt is closed', () => {
		expect(nextReason(false, 'missing', 'rejected')).toBe('rejected');
		expect(nextReason(false, 'rejected', 'missing')).toBe('missing');
	});

	it('stays "rejected" when a signed-in browser is refused again', () => {
		expect(nextReason(true, 'rejected', 'rejected')).toBe('rejected');
	});
});
