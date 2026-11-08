import { describe, expect, it } from 'vitest';
import {
	MODULES,
	NAV,
	PRESETS,
	SETTINGS,
	moduleOfPath,
	normalize,
	presetOf,
	shown,
	toValue
} from './modules.js';

describe('modules', () => {
	it('names the preset a list matches', () => {
		expect(presetOf([])).toBe('home');
		expect(presetOf(['tuning', 'reports', 'agents', 'cupping'])).toBe('enthusiast');
		expect(presetOf([...MODULES])).toBe('roastery');
		expect(presetOf(['reports', 'cupping', 'tuning'])).toBe('custom');
	});

	it('keeps known modules in order; none stored is every module', () => {
		expect(normalize(['labels', 'stats', 'reports'])).toEqual(['reports', 'labels']);
		expect(normalize(null)).toEqual([...MODULES]);
		expect(toValue(['costs', 'cupping'])).toBe('["cupping","costs"]');
	});

	it('Home shows the core sections only', () => {
		expect(shown(NAV, PRESETS.home).map((n) => n.label)).toEqual([
			'Roast',
			'History',
			'Profiles',
			'Beans',
			'Roasters',
			'Settings'
		]);
		expect(shown(NAV, PRESETS.roastery)).toHaveLength(NAV.length);
		expect(shown(NAV, ['reports']).map((n) => n.label)).toContain('Reports');
	});

	it('groups tokens, approvals and audit under Agents & access, gone when agents is off', () => {
		const on = shown(SETTINGS, PRESETS.enthusiast);
		const i = on.findIndex((s) => s.group === 'Agents & access');
		expect(on.slice(i, i + 3).map((s) => s.label)).toEqual(['Tokens', 'Approvals', 'Audit']);
		const home = shown(SETTINGS, PRESETS.home).map((s) => s.label);
		expect(home).not.toContain('Tokens');
		expect(home).toContain('Modules');
	});

	it('knows which pages belong to a module', () => {
		expect(moduleOfPath('/labels/designs/x')).toBe('labels');
		expect(moduleOfPath('/history/abc/labels')).toBe('labels');
		expect(moduleOfPath('/settings/labels')).toBe('labels');
		expect(moduleOfPath('/reports')).toBe('reports');
		expect(moduleOfPath('/history/abc/report')).toBe('reports');
		expect(moduleOfPath('/settings/audit/')).toBe('agents');
		expect(moduleOfPath('/roasters/3/autotune')).toBe('tuning');
		expect(moduleOfPath('/roasters/3/control')).toBeNull();
		expect(moduleOfPath('/history/abc')).toBeNull();
		expect(moduleOfPath('/labelsx')).toBeNull();
	});
});
