/**
 * Where each top-bar section's link goes: back to the page last open in that section during
 * this browser tab (a bean, a roast, a Settings page), as a phone's tab bar does. The link of
 * the section you are in goes to the section's own page.
 */

const SECTION_ROOTS = [
	'/history',
	'/profiles',
	'/beans',
	'/labels',
	'/reports',
	'/roasters',
	'/settings'
] as const;

/** The section a path belongs to ('/' for the Roast page and anything else). */
export function sectionOf(pathname: string): string {
	return SECTION_ROOTS.find((r) => pathname === r || pathname.startsWith(`${r}/`)) ?? '/';
}

/** Pages that should not be returned to: forms that are done once they are left. */
const NOT_KEPT = [/\/new$/, /\/edit$/, /^\/history\/[^/]+\/report$/];

/** `last` with this page remembered for its section. */
export function remember(
	last: Record<string, string>,
	pathname: string,
	search = ''
): Record<string, string> {
	const section = sectionOf(pathname);
	if (section === '/' || NOT_KEPT.some((re) => re.test(pathname))) return last;
	const at = pathname + search;
	return last[section] === at ? last : { ...last, [section]: at };
}

/** The link for a section's tab, seen from `currentPath`. */
export function sectionHref(
	last: Record<string, string>,
	section: string,
	currentPath: string
): string {
	if (section === '/' || sectionOf(currentPath) === section) return section;
	return last[section] ?? section;
}
