import { describe, it, expect } from 'vitest';
import { resolveTheme } from './theme.svelte.js';

describe('theme resolution', () => {
	it('follows the system only when the preference is system', () => {
		expect(resolveTheme('system', 'light')).toBe('light');
		expect(resolveTheme('system', 'dark')).toBe('dark');
		expect(resolveTheme('light', 'dark')).toBe('light');
		expect(resolveTheme('dark', 'light')).toBe('dark');
	});
});
