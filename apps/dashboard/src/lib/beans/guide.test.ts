import { describe, expect, it } from 'vitest';
import type { BeanGuide } from '#lib/api/beans.js';
import { firstRoastLine, guideHeading, guideText } from './guide.js';

const guide = (over: Partial<BeanGuide> = {}): BeanGuide => ({
	bean_id: 'b',
	bean: 'Burundi Dry Process Kibingo',
	stage: 'before',
	roaster: 'esp32_roaster_01',
	profile_id: 'fluidbed-espresso-short',
	profile: 'Fluid bed — Light espresso',
	after: null,
	reads: {
		process: 'natural',
		process_as_given: 'Dry',
		peaberry: false,
		supplier: "Sweet Maria's",
		level: 'City to Full City+',
		espresso: null,
		density_g_l: null,
		moisture_pct: null
	},
	roasts: 0,
	roasts_on_profile: 0,
	first_crack: {
		bt: 195.2,
		window_c: 4,
		t: 469.2,
		window_s: 30,
		basis: '12 roasts on Fluid bed — Light espresso'
	},
	lines: [{ key: 'b1', text: 'Start from your espresso profile.', source: 'rule 1', kind: 'line' }],
	tasting: null,
	...over
});

describe('firstRoastLine', () => {
	it('says where first crack is expected, in the display unit', () => {
		expect(firstRoastLine(guide())).toBe(
			'New bean: first crack near 195 ± 4 °C, about 7:49 ± 30 s'
		);
		const f = {
			temp: (c: number) => ((c * 9) / 5 + 32).toFixed(0),
			delta: (c: number) => ((c * 9) / 5).toFixed(0),
			unit: '°F'
		};
		expect(firstRoastLine(guide(), f)).toBe(
			'New bean: first crack near 383 ± 7 °F, about 7:49 ± 30 s'
		);
	});
	it('leaves the time out without a profile', () => {
		const g = guide({
			profile: null,
			profile_id: null,
			first_crack: {
				bt: 195.2,
				window_c: 4,
				t: null,
				window_s: null,
				basis: '13 roasts on this roaster'
			}
		});
		expect(firstRoastLine(g)).toBe('New bean: first crack near 195 ± 4 °C');
	});
	it('says nothing once the bean has a roast', () => {
		expect(firstRoastLine(guide({ stage: 'next' }))).toBeNull();
		expect(firstRoastLine(null)).toBeNull();
	});
	it('says so when nothing on the roaster marked first crack', () => {
		expect(firstRoastLine(guide({ first_crack: null }))).toMatch(/no first crack marked/);
	});
});

describe('guideHeading and guideText', () => {
	it('names the stage, and none when settled', () => {
		expect(guideHeading(guide())).toBe('First roast');
		expect(guideHeading(guide({ stage: 'next' }))).toBe('Next roast of this bean');
		expect(guideHeading(guide({ stage: 'settled' }))).toBeNull();
	});
	it('joins the lines and the tasting hint', () => {
		expect(guideText(guide({ tasting: 'If it tastes sour…' }))).toBe(
			'Start from your espresso profile. If it tastes sour…'
		);
	});
});
