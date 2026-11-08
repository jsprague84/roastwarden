/**
 * The public demo's links (docs/DEPLOY.md, 'A public demo'): where to install your own, how to
 * send feedback, the discussions. Shown only when the server says it is a demo.
 */
import { NAV } from '#lib/modules/modules.js';
import { sectionOf } from '#lib/nav/sections.js';

export const FEEDBACK_EMAIL = 'hello@roastwarden.com';
export const INSTALL_URL = 'https://roastwarden.com/docs/install/';
export const SITE_URL = 'https://roastwarden.com/';
export const DISCUSSIONS_URL = 'https://github.com/jsprague84/roastwarden/discussions';

/** The page's name for a mail's subject: its section ("Roast page", "Labels"). */
export function pageName(pathname: string): string {
	const section = sectionOf(pathname);
	const label = NAV.find((n) => n.href === section)?.label ?? 'Roast';
	return section === '/' ? 'Roast page' : label;
}

/** A mail to the project about the demo, its subject naming the page the visitor is on. */
export function feedbackHref(pathname: string): string {
	const subject = `Roastwarden demo: ${pageName(pathname)}`;
	return `mailto:${FEEDBACK_EMAIL}?subject=${encodeURIComponent(subject)}`;
}
