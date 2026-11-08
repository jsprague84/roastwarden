/**
 * The classic SCA cupping form (2004 protocol; docs/cupping.md), as the server scores it.
 *
 * Seven attributes score quality, 0–10 in quarter points (the form's scale runs 6–9.75).
 * Uniformity, clean cup and sweetness give 2 points a cup over five cups and are normally 10.
 * Defects take 2 points for each tainted cup and 4 for each faulty one.
 * Final = the ten's total − defects.
 */

export const CUPS = 5;

type AttributeKind = 'quality' | 'cups';

export interface CuppingAttributeDef {
	key: string;
	label: string;
	kind: AttributeKind;
}

/** In the protocol's order. */
export const ATTRIBUTES: readonly CuppingAttributeDef[] = [
	{ key: 'fragrance_aroma', label: 'Fragrance/Aroma', kind: 'quality' },
	{ key: 'flavor', label: 'Flavor', kind: 'quality' },
	{ key: 'aftertaste', label: 'Aftertaste', kind: 'quality' },
	{ key: 'acidity', label: 'Acidity', kind: 'quality' },
	{ key: 'body', label: 'Body', kind: 'quality' },
	{ key: 'balance', label: 'Balance', kind: 'quality' },
	{ key: 'uniformity', label: 'Uniformity', kind: 'cups' },
	{ key: 'clean_cup', label: 'Clean Cup', kind: 'cups' },
	{ key: 'sweetness', label: 'Sweetness', kind: 'cups' },
	{ key: 'overall', label: 'Overall', kind: 'quality' }
];

/** Where a fresh form starts: 6.00 (the scale's start) and every cup passing. */
function startScore(kind: AttributeKind): number {
	return kind === 'cups' ? 10 : 6;
}

export function freshScores(): Record<string, number> {
	return Object.fromEntries(ATTRIBUTES.map((a) => [a.key, startScore(a.kind)]));
}

/** Cups that pass, for a cup attribute's score (2 a cup). */
export function cupsFromScore(score: number): number {
	return Math.max(0, Math.min(CUPS, Math.round(score / 2)));
}

export function scoreFromCups(cups: number): number {
	return 2 * Math.max(0, Math.min(CUPS, cups));
}

/** Points off: 2 per tainted cup, 4 per faulty cup. */
export function defects(taintCups: number, faultCups: number): number {
	return 2 * taintCups + 4 * faultCups;
}

/** The ten attributes' sum, or null until all ten are scored. */
export function total(scores: Record<string, number | undefined>): number | null {
	let sum = 0;
	for (const a of ATTRIBUTES) {
		const s = scores[a.key];
		if (s == null || Number.isNaN(s)) return null;
		sum += s;
	}
	return sum;
}

export type Band = 'outstanding' | 'excellent' | 'very good' | 'below specialty';

export function band(finalScore: number): Band {
	if (finalScore >= 90) return 'outstanding';
	if (finalScore >= 85) return 'excellent';
	if (finalScore >= 80) return 'very good';
	return 'below specialty';
}

export interface CuppingSummary {
	total: number | null;
	defects: number;
	final: number | null;
	band: Band | null;
}

export function summarize(
	scores: Record<string, number | undefined>,
	taintCups: number,
	faultCups: number
): CuppingSummary {
	const t = total(scores);
	const d = defects(taintCups, faultCups);
	const f = t == null ? null : t - d;
	return { total: t, defects: d, final: f, band: f == null ? null : band(f) };
}

/** Taint and fault cups together can be at most the five cups: the other is capped. */
export function capDefects(
	changed: 'taint' | 'fault',
	taintCups: number,
	faultCups: number
): { taint: number; fault: number } {
	const t = Math.max(0, Math.min(CUPS, taintCups));
	const f = Math.max(0, Math.min(CUPS, faultCups));
	if (t + f <= CUPS) return { taint: t, fault: f };
	return changed === 'taint' ? { taint: t, fault: CUPS - t } : { taint: CUPS - f, fault: f };
}
