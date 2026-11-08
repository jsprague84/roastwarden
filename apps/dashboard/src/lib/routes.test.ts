import { describe, it, expect } from 'vitest';
import { legacyRoasterPath } from './routes.js';

const q = (s = '') => new URLSearchParams(s);

describe('legacyRoasterPath', () => {
	it('maps the list, new and templates', () => {
		expect(legacyRoasterPath('', q())).toBe('/roasters');
		expect(legacyRoasterPath('new', q('device_id=virtual_roaster'))).toBe(
			'/roasters/new?device_id=virtual_roaster'
		);
		expect(legacyRoasterPath('profiles', q())).toBe('/roasters/templates');
	});

	it('turns the old tab query into a sub-page', () => {
		expect(legacyRoasterPath('abc', q())).toBe('/roasters/abc');
		expect(legacyRoasterPath('abc', q('tab=control'))).toBe('/roasters/abc/control');
		expect(legacyRoasterPath('abc', q('tab=autotune&x=1'))).toBe('/roasters/abc/autotune?x=1');
		expect(legacyRoasterPath('abc', q('tab=overview'))).toBe('/roasters/abc');
	});
});
