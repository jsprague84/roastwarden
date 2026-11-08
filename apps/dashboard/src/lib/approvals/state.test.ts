import { describe, expect, it } from 'vitest';
import { expiresIn, isOpen, outcome } from './state.js';

const now = Date.parse('2026-10-25T12:00:00Z');
const base = {
	result_status: null,
	decided_at: null,
	expires_at: '2026-10-25T12:42:00Z',
	result: null
};

describe('approvals in words', () => {
	it('says how long a request still waits', () => {
		expect(expiresIn('2026-10-25T12:42:00Z', now)).toBe('expires in 42 min');
		expect(expiresIn('2026-10-25T12:01:00Z', now)).toBe('expires in 1 min');
		expect(expiresIn('2026-10-25T11:59:00Z', now)).toBe('expires now');
	});
	it('says what became of it', () => {
		expect(outcome({ ...base, status: 'pending' }, now)).toBe('Waiting · expires in 42 min');
		expect(outcome({ ...base, status: 'done', decided_at: '2026-10-25T11:30:00Z' }, now)).toBe(
			'Approved and done 30 min ago'
		);
		expect(outcome({ ...base, status: 'denied', decided_at: '2026-10-25T11:59:30Z' }, now)).toBe(
			'Denied just now · nothing was done'
		);
		expect(
			outcome(
				{
					...base,
					status: 'failed',
					decided_at: '2026-10-25T11:59:30Z',
					result_status: 410,
					result: { error: { message: 'The token was revoked' } }
				},
				now
			)
		).toBe('Approved just now, but the call failed (410: The token was revoked)');
		expect(outcome({ ...base, status: 'expired' }, now)).toContain('nothing was done');
	});
	it('knows which are open', () => {
		expect(isOpen({ status: 'pending' })).toBe(true);
		expect(isOpen({ status: 'expired' })).toBe(false);
	});
});
