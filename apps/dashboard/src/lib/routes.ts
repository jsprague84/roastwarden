/**
 * URL helpers for pages that moved (docs/ui-structure.md, "URLs").
 */

const ROASTER_TABS = new Set(['control', 'autotune', 'simulator']);

/**
 * Where an old `/devices/...` URL lives now: `/devices/profiles` became
 * `/roasters/templates`, and the roaster page's `?tab=x` became `/roasters/<id>/x`.
 */
export function legacyRoasterPath(rest: string, search: URLSearchParams): string {
	const parts = rest.split('/').filter(Boolean);
	const q = new URLSearchParams(search);
	if (parts[0] === 'profiles') parts[0] = 'templates';
	const tab = q.get('tab');
	q.delete('tab');
	if (
		parts.length === 1 &&
		parts[0] !== 'new' &&
		parts[0] !== 'templates' &&
		tab &&
		ROASTER_TABS.has(tab)
	) {
		parts.push(tab);
	}
	const qs = q.toString();
	return `/roasters${parts.length ? `/${parts.join('/')}` : ''}${qs ? `?${qs}` : ''}`;
}
