/**
 * Modules (docs/lean-home.md): the optional parts of the dashboard. The server keeps the list
 * that is on (the `modules` setting, read from `GET /api/about`); an off module leaves the nav,
 * the Settings menu and the buttons that lead to it. Its API stays.
 */

import type { RouteId } from '$app/types';

export const MODULES = ['reports', 'cupping', 'tuning', 'agents', 'costs', 'labels'] as const;
export type Module = (typeof MODULES)[number];

export const PRESETS = {
	home: [] as Module[],
	enthusiast: ['reports', 'cupping', 'tuning', 'agents'] as Module[],
	roastery: [...MODULES] as Module[]
};
export type Preset = keyof typeof PRESETS;

/** Each module's name, what it shows (one line), and the line its pages show when it is off. */
export const MODULE_INFO: Record<Module, { name: string; line: string; off: string }> = {
	reports: {
		name: 'Reports',
		line: 'PDF reports: the Reports section and a roast’s Report button.',
		off: 'Reports are off'
	},
	cupping: {
		name: 'Cupping',
		line: 'The cupping form and its scores on a roast.',
		off: 'Cupping is off'
	},
	tuning: {
		name: 'Tuning',
		line: 'Autotune, the gain schedule and fan assist on a roaster’s pages.',
		off: 'Tuning is off'
	},
	agents: {
		name: 'Agents & access',
		line: 'API tokens, approvals and the audit log, for scripts and agents.',
		off: 'Agents & access is off'
	},
	costs: {
		name: 'Costs',
		line: 'Prices on purchases, cost per kg and per roast, spending and the shopping list.',
		off: 'Costs are off'
	},
	labels: {
		name: 'Labels',
		line: 'Bag labels, Label Studio and the share pages their QR codes open.',
		off: 'Labels are off'
	}
};

export const PRESET_INFO: Record<Preset, { name: string; line: string }> = {
	home: { name: 'Home', line: 'Roast, history, profiles, beans and roasters. Nothing else.' },
	enthusiast: { name: 'Enthusiast', line: 'Adds reports, cupping, tuning, and tokens for agents.' },
	roastery: { name: 'Roastery', line: 'Everything: adds costs and labels for selling coffee.' }
};

/** The modules a stored list names, in their order; none given is every module. */
export function normalize(list: readonly string[] | null | undefined): Module[] {
	if (!list) return [...MODULES];
	return MODULES.filter((m) => list.includes(m));
}

/** The preset a list is, or `custom`. */
export function presetOf(on: readonly Module[]): Preset | 'custom' {
	for (const [name, list] of Object.entries(PRESETS) as [Preset, Module[]][]) {
		if (list.length === on.length && list.every((m) => on.includes(m))) return name;
	}
	return 'custom';
}

/** The module a page belongs to, or null for a core page. */
export function moduleOfPath(pathname: string): Module | null {
	const p = pathname.replace(/\/+$/, '') || '/';
	if (p === '/reports' || p.startsWith('/reports/') || /^\/history\/[^/]+\/report$/.test(p))
		return 'reports';
	if (
		p === '/labels' ||
		p.startsWith('/labels/') ||
		/^\/history\/[^/]+\/labels$/.test(p) ||
		p === '/settings/labels'
	)
		return 'labels';
	if (['/settings/tokens', '/settings/approvals', '/settings/audit'].includes(p)) return 'agents';
	if (/^\/roasters\/[^/]+\/autotune$/.test(p)) return 'tuning';
	return null;
}

/** The items whose module is on (items without one always stay). */
export function shown<T extends { module?: Module }>(
	items: readonly T[],
	on: readonly Module[]
): T[] {
	return items.filter((i) => !i.module || on.includes(i.module));
}

/** The setting's value for a list. */
export function toValue(on: readonly Module[]): string {
	return JSON.stringify(normalize(on));
}

/** A page with no parameter in its route, the kind a menu links to (svelte-check checks each). */
type MenuRoute = Exclude<RouteId, `${string}[${string}`>;

/** The top bar's sections; Labels and Reports are modules. */
export const NAV: readonly { href: MenuRoute; label: string; hint: string; module?: Module }[] = [
	{ href: '/', label: 'Roast', hint: 'The roast in progress' },
	{ href: '/history', label: 'History', hint: 'Finished roasts and comparisons' },
	{
		href: '/profiles',
		label: 'Profiles',
		hint: 'Roast profiles: library, designer, Artisan import and export'
	},
	{ href: '/beans', label: 'Beans', hint: 'Roasted coffee, planned roasts and green stock' },
	{
		href: '/labels',
		label: 'Labels',
		hint: 'Label Studio: bag label designs, printing, artwork and brands',
		module: 'labels'
	},
	{
		href: '/reports',
		label: 'Reports',
		hint: 'PDF reports: a roast, a coffee, a profile, a period, cupping sheets, stock, purchases and the roast-day plan',
		module: 'reports'
	},
	{
		href: '/roasters',
		label: 'Roasters',
		hint: 'Each roaster: connection, control, check, simulator'
	},
	{ href: '/settings', label: 'Settings', hint: 'Display, roast defaults, alarms, modules' }
];

/** Settings' menu; `group` puts a heading above the entry ("Agents & access"). */
export const SETTINGS: readonly {
	href: MenuRoute;
	label: string;
	module?: Module;
	group?: string;
}[] = [
	{ href: '/settings', label: 'This browser' },
	{ href: '/settings/roasting', label: 'Roasting' },
	{ href: '/settings/alarms', label: 'Alarms' },
	{ href: '/settings/notifications', label: 'Notifications' },
	{ href: '/settings/home-assistant', label: 'Home Assistant' },
	{ href: '/settings/backups', label: 'Backups' },
	{ href: '/settings/modules', label: 'Modules' },
	{ href: '/settings/tokens', label: 'Tokens', module: 'agents', group: 'Agents & access' },
	{ href: '/settings/approvals', label: 'Approvals', module: 'agents' },
	{ href: '/settings/audit', label: 'Audit', module: 'agents' },
	{ href: '/settings/about', label: 'About' }
];
