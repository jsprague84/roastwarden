/**
 * Why the sign-in prompt is open (SECURITY.md, "Logins"): `missing`, this browser has not signed
 * in, so with reads closed every request of the page was refused and signing in loads the page
 * again; `rejected`, a sign-in this browser had stopped working, and the page keeps what is on it.
 */
export type PromptReason = 'missing' | 'rejected';

/**
 * The reason once the prompt is asked for again. A refusal that arrives while the prompt is open
 * because this browser never signed in keeps `missing`: the page's first requests are refused in
 * no fixed order around the sign-in check, and only `missing` reloads the page after signing in
 * (which also opens a new install's first-run page).
 */
export function nextReason(
	open: boolean,
	current: PromptReason,
	asked: PromptReason
): PromptReason {
	return open && current === 'missing' ? 'missing' : asked;
}
