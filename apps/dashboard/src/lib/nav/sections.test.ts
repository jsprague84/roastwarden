import { describe, expect, it } from 'vitest';
import { remember, sectionHref, sectionOf } from './sections.js';

describe('section links', () => {
	it('knows the section of a path', () => {
		expect(sectionOf('/')).toBe('/');
		expect(sectionOf('/beans/abc')).toBe('/beans');
		expect(sectionOf('/beansy')).toBe('/');
		expect(sectionOf('/settings/roasting')).toBe('/settings');
		expect(sectionOf('/reports')).toBe('/reports');
	});

	it('goes back to the page last open in another section', () => {
		let last: Record<string, string> = {};
		last = remember(last, '/beans/abc');
		last = remember(last, '/history', '?x=1');
		expect(sectionHref(last, '/beans', '/')).toBe('/beans/abc');
		expect(sectionHref(last, '/history', '/beans/abc')).toBe('/history?x=1');
		expect(sectionHref(last, '/profiles', '/')).toBe('/profiles');
	});

	it("goes to the section's own page from inside it", () => {
		const last = remember({}, '/beans/abc');
		expect(sectionHref(last, '/beans', '/beans/abc')).toBe('/beans');
		expect(sectionHref(last, '/', '/beans/abc')).toBe('/');
	});

	it('does not return to forms', () => {
		let last = remember({}, '/profiles/p1');
		last = remember(last, '/profiles/p1/edit');
		last = remember(last, '/profiles/new');
		expect(last['/profiles']).toBe('/profiles/p1');
		expect(remember({}, '/history/r1/report')).toEqual({});
	});

	it('keeps the same object when nothing changed', () => {
		const last = remember({}, '/beans/abc');
		expect(remember(last, '/beans/abc')).toBe(last);
		expect(remember(last, '/')).toBe(last);
	});
});
