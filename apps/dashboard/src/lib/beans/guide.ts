/**
 * A bean's guide (docs/new-bean.md): what the dashboard says from the server's
 * `GET /api/beans/{id}/guide`. The server writes the lines; this only shapes the
 * Roast page's one line and the section's heading.
 */
import type { BeanGuide } from '#lib/api/beans.js';
import { fmtClock } from '#lib/scope/scopeData.js';

export interface Units {
	/** °C → display, no unit */
	temp: (c: number) => string;
	/** °C difference → display, no unit */
	delta: (c: number) => string;
	unit: string;
}

const C: Units = { temp: (c) => c.toFixed(0), delta: (c) => c.toFixed(0), unit: '°C' };

/** The step card's one line before a bean's first roast; null otherwise. */
export function firstRoastLine(g: BeanGuide | null, u: Units = C): string | null {
	if (!g || g.stage !== 'before') return null;
	const fc = g.first_crack;
	if (!fc) return 'New bean: no first crack marked on this roaster yet to expect it by.';
	const when =
		fc.t != null
			? `, about ${fmtClock(fc.t)}${fc.window_s != null ? ` ± ${fc.window_s} s` : ''}`
			: '';
	return `New bean: first crack near ${u.temp(fc.bt)} ± ${u.delta(fc.window_c)} ${u.unit}${when}`;
}

/** The section's heading: "First roast", "Next roast of this bean", or null (settled). */
export function guideHeading(g: BeanGuide | null): string | null {
	if (!g) return null;
	if (g.stage === 'before') return 'First roast';
	if (g.stage === 'next') return 'Next roast of this bean';
	return null;
}

/** The whole guide as text, for the step card's Hint. */
export function guideText(g: BeanGuide): string {
	return [...g.lines.map((l) => l.text), ...(g.tasting ? [g.tasting] : [])].join(' ');
}
