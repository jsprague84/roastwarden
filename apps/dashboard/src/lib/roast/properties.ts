/** Roast properties (migration 016): choices and formatting shared by the session page and lists. */

export const PURPOSES = [
	{ value: 'espresso', label: 'Espresso' },
	{ value: 'filter', label: 'Filter' },
	{ value: 'omni', label: 'Omni' },
	{ value: 'other', label: 'Other' }
] as const;

export const COLOR_SCALES = [
	{ value: 'agtron', label: 'Agtron (gourmet)' },
	{ value: 'tonino', label: 'Tonino' },
	{ value: 'lebrew', label: 'Lebrew RoastSee' },
	{ value: 'other', label: 'Other' }
] as const;

export const ROAST_FLAGS = [
	{ value: 'underdeveloped', label: 'Underdeveloped' },
	{ value: 'baked', label: 'Baked / flat' },
	{ value: 'scorching', label: 'Scorching' },
	{ value: 'tipping', label: 'Tipping' },
	{ value: 'uneven', label: 'Uneven' },
	{ value: 'stall', label: 'RoR stall/crash' },
	{ value: 'flick', label: 'RoR flick' }
] as const;

export function parseFlags(s: string | null | undefined): string[] {
	return (s ?? '')
		.split(',')
		.map((f) => f.trim())
		.filter(Boolean);
}

export function joinFlags(flags: string[]): string | null {
	const uniq = [...new Set(flags.map((f) => f.trim()).filter(Boolean))];
	return uniq.length ? uniq.join(',') : null;
}

export function flagLabel(f: string): string {
	return ROAST_FLAGS.find((x) => x.value === f)?.label ?? f;
}

export function purposeLabel(p: string | null | undefined): string {
	return PURPOSES.find((x) => x.value === p)?.label ?? p ?? '';
}

/** "Agtron 68 / 74" (whole / ground), or null. */
export function colorText(
	whole: number | null | undefined,
	ground: number | null | undefined,
	scale: string | null | undefined
): string | null {
	if (whole == null && ground == null) return null;
	const name = scale
		? (COLOR_SCALES.find((x) => x.value === scale)?.label.split(' ')[0] ?? scale)
		: 'Colour';
	const parts = [whole != null ? `${whole}` : '–', ground != null ? `${ground}` : '–'];
	return `${name} ${parts.join(' / ')}`;
}

/** Field value from a text input: '' → null (clears on the server), else a number. */
export function numOrNull(v: string | number | null | undefined): number | null {
	if (v == null) return null;
	const s = String(v).trim();
	if (s === '') return null;
	const n = Number(s);
	return Number.isFinite(n) ? n : null;
}

/**
 * Field value for a field the server keeps when absent (the weights): blank → undefined.
 * Takes what a Svelte 5 `type="number"` input binds: a number once typed, null when cleared,
 * or the string the page filled it with.
 */
export function numOrUndefined(v: string | number | null | undefined): number | undefined {
	return numOrNull(v) ?? undefined;
}
