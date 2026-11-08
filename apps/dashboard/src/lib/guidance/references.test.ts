import { describe, expect, it } from 'vitest';
import {
	defaultsFor,
	densityClass,
	greenMoistureRange,
	levelForAgtron,
	levelForLoss,
	moistureCorrected,
	toAgtron,
	LEVELS,
	REFERENCES,
	SOURCES
} from './references.js';

describe('references', () => {
	it('every reference names known sources and every source is used', () => {
		const ids = new Set(SOURCES.map((s) => s.id));
		expect(ids.size).toBe(SOURCES.length);
		const used = new Set<string>();
		for (const r of REFERENCES) {
			expect(r.sources.length, r.metric).toBeGreaterThan(0);
			for (const s of r.sources) {
				expect(ids.has(s), `${r.metric} → ${s}`).toBe(true);
				used.add(s);
			}
		}
		for (const s of SOURCES) expect(used.has(s.id), `${s.id} unused`).toBe(true);
	});

	it('sources carry a url and a grade', () => {
		for (const s of SOURCES) {
			expect(s.url, s.id).toMatch(/^https:\/\//);
			expect(['a', 'b', 'c']).toContain(s.type);
		}
	});

	it('derived defaults say so', () => {
		const d = defaultsFor('light');
		expect(d.weight_loss_pct?.derived).toBe(true);
		expect(d.development_s?.derived).toBe(true);
		expect(d.dtr_pct?.derived).toBe(true);
		expect(defaultsFor('medium_light').weight_loss_pct?.derived).toBe(false);
		for (const r of REFERENCES.filter((x) => x.derived))
			expect(r.basis.length, r.metric).toBeGreaterThan(10);
	});

	it('ranges per level are ordered and get darker with the level', () => {
		let prevLoss = -Infinity;
		let prevAgtron = Infinity;
		for (const l of LEVELS) {
			const d = defaultsFor(l.key);
			for (const r of Object.values(d)) if (r) expect(r.range[0]).toBeLessThanOrEqual(r.range[1]);
			expect(d.weight_loss_pct!.range[0]).toBeGreaterThanOrEqual(prevLoss);
			prevLoss = d.weight_loss_pct!.range[0];
			expect(l.agtron[1]).toBeLessThanOrEqual(prevAgtron);
			prevAgtron = l.agtron[0];
		}
		expect(defaultsFor(null).weight_loss_pct).toBeNull();
		// No source covers development for dark roasts: no default rather than a guess.
		expect(defaultsFor('dark').development_s).toBeNull();
	});

	it('Tonino conversion hits the maker’s points', () => {
		expect(toAgtron(105, 'tonino').agtron).toBeCloseTo(63.3, 1);
		expect(toAgtron(115, 'tonino').agtron).toBe(70);
		expect(toAgtron(125, 'tonino').agtron).toBe(80);
		expect(toAgtron(85, 'tonino').agtron).toBe(50);
		expect(toAgtron(140, 'tonino').agtron).toBe(90);
		expect(toAgtron(140, 'tonino').approximate).toBe(true);
		expect(toAgtron(72, 'agtron')).toEqual({ agtron: 72, approximate: false, note: '' });
		expect(toAgtron(72, 'lebrew').approximate).toBe(true);
		expect(toAgtron(72, 'other').agtron).toBeNull();
		expect(toAgtron(null, 'agtron').agtron).toBeNull();
	});

	it('weight loss maps back to a level', () => {
		expect(levelForLoss(21.7)).toEqual({ level: 'dark', agtron: [25, 35] });
		expect(levelForLoss(15).level).toBe('light');
		expect(levelForLoss(13).level).toBe('very_light');
		// Wetter green coffee loses more for the same colour.
		expect(levelForLoss(16, 12.5).level).toBe('very_light');
		expect(levelForAgtron(95)).toBe('extremely_light');
		expect(levelForAgtron(63)).toBe('medium_light');
	});

	it('moisture shifts the loss range', () => {
		expect(moistureCorrected([14, 16], 12)).toEqual([15.5, 17.5]);
		expect(moistureCorrected([14, 16], 9.5)).toEqual([13, 15]);
		expect(moistureCorrected([14, 16], null)).toEqual([14, 16]);
	});

	it('classes green density and bounds moisture', () => {
		expect(densityClass(640)).toBe('low');
		expect(densityClass(670)).toBe('average');
		expect(densityClass(700)).toBe('dense');
		expect(greenMoistureRange().range).toEqual([8, 12.5]);
	});
});
