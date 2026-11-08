/** Print runs (docs/labels.md, "Print runs"): a sheet of labels for one roast or several
 *  (a gift run), its items, where they land on the sheet, and the next free label. */

export type RunItemAsk = {
	roast: string;
	/** For the page: the roast's name and batch */
	name: string;
	count: number;
	/** A design id (or id@N); empty: the coffee's own, else plain */
	design: string;
	message: string;
	/** Net grams on the label; null: the roast's roasted weight */
	weight_g: number | null;
	/** The design's parameters that differ from where it starts (its defaults, or the
	 *  coffee's own label's) */
	params?: Record<string, unknown>;
	/** A brand kit's id; empty: the coffee's own, else Settings → Roasting's */
	brand?: string;
	/** Tasting notes instead of the cupping's or the bean's */
	notes?: string;
	/** Fields left off */
	hide?: string[];
	/** Ground coffee (the label reads "Ground · espresso"), else whole bean */
	ground?: boolean;
	/** The grind, when ground */
	grind?: string;
	/** Take the bags off the roast when the run is made (docs/roasted.md, "Packed bags") */
	pack?: boolean;
};

/** A new item for a roast, as both label pages start one */
export function newItem(roast: string, name: string, o: Partial<RunItemAsk> = {}): RunItemAsk {
	return {
		roast,
		name,
		count: 1,
		design: '',
		message: '',
		weight_g: null,
		params: {},
		brand: '',
		notes: '',
		hide: [],
		...o
	};
}

/** The body of POST /api/label-runs and /preview */
export function runBody(
	items: RunItemAsk[],
	o: { sheet: string; start: number; note?: string; printer?: string; qr?: string },
	tzMinutes = -new Date().getTimezoneOffset()
) {
	return {
		sheet: o.sheet,
		start: o.start,
		tz: tzMinutes,
		note: o.note?.trim() || undefined,
		printer: o.printer || undefined,
		items: items.map((i) => ({
			roast: i.roast,
			count: Math.max(1, Math.round(i.count) || 1),
			design: i.design || undefined,
			message: i.message.trim() || undefined,
			weight_g: i.weight_g && i.weight_g > 0 ? i.weight_g : undefined,
			params: i.params && Object.keys(i.params).length ? i.params : undefined,
			brand: i.brand || undefined,
			notes: i.notes?.trim() || undefined,
			hide: i.hide?.length ? i.hide : undefined,
			form: i.ground ? 'ground' : undefined,
			grind: (i.ground && i.grind?.trim()) || undefined,
			pack: i.pack || undefined,
			qr: o.qr || undefined
		}))
	};
}

/** "Take 3 × 250 g off this roast": the bags a print takes off (copies of the label's
 *  weight, else the roasted weight), and whether that much is left. None without a weight. */
export function packOffer(
	count: number,
	weightG: number | null | undefined,
	roastedG: number | null | undefined,
	leftG: number | null | undefined,
	/** "this roast", or "the bag #16+17" for roasts in one bag */
	what = 'this roast'
): { bags: number; bagG: number; text: string; enough: boolean } | null {
	const bags = Math.max(1, Math.round(count) || 1);
	const bagG = weightG && weightG > 0 ? weightG : roastedG && roastedG > 0 ? roastedG : null;
	if (bagG == null) return null;
	const enough = leftG != null && bags * bagG <= leftG + 0.5;
	return { bags, bagG, text: `Take ${bags} × ${Math.round(bagG * 10) / 10} g off ${what}`, enough };
}

/** How many labels the items make */
export function labelCount(items: { count: number }[]): number {
	return items.reduce((n, i) => n + Math.max(1, Math.round(i.count) || 1), 0);
}

/** Which item each label of the first page is: index into items, -1 blank (before the
 *  start label), null past the end. `per` labels a sheet. */
export function cellOwners(
	items: { count: number }[],
	start: number,
	per: number
): (number | null)[] {
	const out: (number | null)[] = [];
	for (let i = 1; i < start && out.length < per; i++) out.push(-1);
	items.forEach((it, k) => {
		for (let c = 0; c < Math.max(1, Math.round(it.count) || 1); c++) out.push(k);
	});
	while (out.length < per) out.push(null);
	return out.slice(0, per);
}

/** Sheets the labels need from `start` */
export function sheetsFor(labels: number, start: number, per: number): number {
	return Math.max(1, Math.ceil((start - 1 + labels) / per));
}

/** Whether a roast matches the Add roasts search: every word in its name (and batch) or
 *  its coffee's name, in any order and case; an empty search matches all. */
export function roastMatches(find: string, roastName: string, coffee?: string | null): boolean {
	const hay = `${roastName} ${coffee ?? ''}`.toLowerCase();
	return find
		.toLowerCase()
		.split(/\s+/)
		.filter(Boolean)
		.every((w) => hay.includes(w));
}

/** The next free label once `labels` from `start` are used */
export function nextAfter(start: number, labels: number, per: number): number {
	return ((start - 1 + labels) % per) + 1;
}

/** What a design does with the gift message (docs/labels.md, "Designs"): a design may name
 *  the field (`message: "To"` in its manifest, or `message: false` for none); one with a
 *  text parameter `to` takes the recipient there, so the Message field goes; one that prints
 *  its own "To" (a `to` switch, a `from` line, "To:" in its description) gets "To" with
 *  just the name, so a label never reads "To For Sam". */
export function messageField(manifest: unknown): {
	show: boolean;
	label: string;
	placeholder: string;
	help?: string;
} {
	const m = (manifest ?? {}) as {
		message?: unknown;
		params?: Record<string, unknown>;
		description?: unknown;
	};
	const plain = { show: true, label: 'Message (a gift)', placeholder: 'for Sam, with love' };
	if (m.message === false) return { ...plain, show: false };
	if (typeof m.message === 'string' && m.message.trim())
		return { ...plain, label: m.message.trim() };
	const params = m.params && typeof m.params === 'object' ? m.params : {};
	const to = params.to as unknown;
	const toDefault =
		to && typeof to === 'object' && 'default' in to ? (to as { default: unknown }).default : to;
	if (typeof toDefault === 'string') {
		return { ...plain, show: false, help: "This design's own to field is the recipient." };
	}
	const own =
		typeof toDefault === 'boolean' ||
		'from' in params ||
		/\bTo\b\s*[:/]|message is To/.test(String(m.description ?? ''));
	if (own)
		return {
			show: true,
			label: 'To',
			placeholder: 'Sam',
			help: 'The design prints “To”: give the name.'
		};
	return plain;
}

/** Where a run's labels are on its sheet, in words: "labels 1–3 of 5163" */
export function runPlace(
	run: { start: number; labels: number; sheet: string },
	per: number
): string {
	const end = run.start - 1 + run.labels;
	const span =
		run.labels === 1
			? `label ${run.start}`
			: end <= per
				? `labels ${run.start}–${end}`
				: `${run.labels} labels from label ${run.start}`;
	return `${span} of ${run.sheet}`;
}
