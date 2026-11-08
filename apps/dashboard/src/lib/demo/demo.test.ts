import { describe, expect, it } from 'vitest';
import { feedbackHref, pageName } from './demo.js';

describe('demo feedback', () => {
	it('names the section the visitor is on', () => {
		expect(pageName('/')).toBe('Roast page');
		expect(pageName('/history/abc')).toBe('History');
		expect(pageName('/labels/designs/x')).toBe('Labels');
		expect(pageName('/welcome')).toBe('Roast page');
	});

	it('is a mail with the page in its subject', () => {
		expect(feedbackHref('/reports')).toBe(
			'mailto:hello@roastwarden.com?subject=Roastwarden%20demo%3A%20Reports'
		);
	});
});

describe("a demo visitor's roaster", () => {
	it('is simulated like the shared one', async () => {
		const { isVirtual, isVisitorRoaster, isSimulatedFluidBed } = await import('#lib/roast/live.js');
		expect(isVisitorRoaster('virtual_3fa9c1d2')).toBe(true);
		expect(isVisitorRoaster('virtual_roaster')).toBe(false);
		expect(isVisitorRoaster('virtual_3FA9C1D2')).toBe(false);
		expect(isSimulatedFluidBed('virtual_3fa9c1d2') && isVirtual('virtual_3fa9c1d2')).toBe(true);
		expect(isSimulatedFluidBed('virtual_drum')).toBe(false);
		expect(isVirtual('esp32_roaster_01')).toBe(false);
	});
});
