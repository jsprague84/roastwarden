/** Approval requests in words, for Settings → Approvals (docs/control.md, "Approvals"). */
import type { Approval } from '#lib/api/approvals.js';
import { ago } from '#lib/tokens/state.js';

/** Still waiting for the owner. */
export const isOpen = (a: Pick<Approval, 'status'>) => a.status === 'pending';

/** "expires in 42 min", for a pending request. */
export function expiresIn(iso: string, now = Date.now()): string {
	const m = Math.round((Date.parse(iso) - now) / 60_000);
	if (m <= 0) return 'expires now';
	return m === 1 ? 'expires in 1 min' : `expires in ${m} min`;
}

/** What became of a request, in a few words. */
export function outcome(
	a: Pick<Approval, 'status' | 'result_status' | 'decided_at' | 'expires_at' | 'result'>,
	now = Date.now()
): string {
	switch (a.status) {
		case 'pending':
			return `Waiting · ${expiresIn(a.expires_at, now)}`;
		case 'approved':
			return 'Approved · running';
		case 'done':
			return `Approved and done ${ago(a.decided_at, now)}`;
		case 'failed':
			return `Approved ${ago(a.decided_at, now)}, but the call failed (${a.result_status ?? '?'}${errorOf(a.result)})`;
		case 'denied':
			return `Denied ${ago(a.decided_at, now)} · nothing was done`;
		case 'expired':
			return 'Expired without an answer · nothing was done';
	}
}

function errorOf(result: unknown): string {
	const m = (result as { error?: { message?: unknown } } | null)?.error?.message;
	return typeof m === 'string' && m ? `: ${m}` : '';
}
