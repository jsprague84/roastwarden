/** Bag labels (docs/labels.md): the request a label sheet is made from, and where each label
 *  sits on the sheet's preview so a click can pick the first one to print on. */

export type LabelSheet = {
	id: string;
	name: string;
	page_w: number;
	page_h: number;
	w: number;
	h: number;
	round: boolean;
	cols: number;
	rows: number;
	left: number;
	top: number;
	pitch_x: number;
	pitch_y: number;
	offset: { x_mm: number; y_mm: number };
	/** The next free label on the part-used sheet (1: a new sheet) */
	next_label?: number;
};

/** A design to choose from: saved (with its current version), the owner's folder, or built in */
export type LabelDesign = {
	id: string;
	name: string;
	kind: 'saved' | 'folder' | 'builtin';
	version?: number;
	params?: string[];
	description?: string;
	updated_at?: string;
	/** Coffees whose own label is this design */
	used_by?: string[];
};

/** A bag a sold label goes on: its front's size sets the type height (docs/labels.md,
 *  "The retail check") */
export type LabelBag = { id: string; name: string; front_w: number; front_h: number };

export type LabelOptions = {
	bags?: LabelBag[];
	/** The bag a retail check assumes unless told */
	bag?: string;
	/** What a label's QR code opens unless told (`label_qr`) */
	qr?: string;
	sheets: LabelSheet[];
	designs: LabelDesign[];
	fields: string[];
	rest_days: { espresso_light: number; espresso: number; filter_light: number; filter: number };
	fonts: string[];
	folder: string;
};

export type LabelAsk = {
	sheet: string;
	design: string;
	count: number;
	start: number;
	weight_g?: number | null;
	message?: string;
	notes?: string;
	/** Fields left off */
	hide: string[];
	/** The design's parameters that differ from its defaults */
	params?: Record<string, unknown>;
	/** A brand kit's id */
	brand?: string;
	/** What the QR code opens: app, share, none or an address (docs/labels.md) */
	qr?: string;
};

/** The query string for labels.pdf / labels.svg. Dates are this browser's local dates. */
export function labelQuery(a: LabelAsk, tzMinutes = -new Date().getTimezoneOffset()): string {
	const q = new URLSearchParams({
		sheet: a.sheet,
		design: a.design,
		count: String(Math.max(1, Math.round(a.count))),
		start: String(Math.max(1, Math.round(a.start))),
		tz: String(tzMinutes)
	});
	if (a.weight_g && a.weight_g > 0) q.set('weight_g', String(a.weight_g));
	if (a.message?.trim()) q.set('message', a.message.trim());
	if (a.notes?.trim()) q.set('notes', a.notes.trim());
	if (a.hide.length) q.set('hide', a.hide.join(','));
	if (a.params && Object.keys(a.params).length) q.set('params', JSON.stringify(a.params));
	if (a.brand) q.set('brand', a.brand);
	if (a.qr) q.set('qr', a.qr);
	return q.toString();
}

/** Each label's box on the page, in % of the page, in print order (row by row). */
export function slotBoxes(
	s: LabelSheet
): { left: number; top: number; width: number; height: number }[] {
	const out = [];
	for (let r = 0; r < s.rows; r++) {
		for (let c = 0; c < s.cols; c++) {
			out.push({
				left: ((s.left + c * s.pitch_x) / s.page_w) * 100,
				top: ((s.top + r * s.pitch_y) / s.page_h) * 100,
				width: (s.w / s.page_w) * 100,
				height: (s.h / s.page_h) * 100
			});
		}
	}
	return out;
}

/** How many sheets `count` labels from label `start` take. */
export function sheetsNeeded(s: LabelSheet, count: number, start: number): number {
	const per = s.cols * s.rows;
	return Math.max(1, Math.ceil((Math.max(1, start) - 1 + Math.max(1, count)) / per));
}

/** Words for the fields a label can leave off. */
export const FIELD_LABELS: Record<string, string> = {
	origin: 'Origin',
	variety: 'Variety',
	process: 'Process',
	level: 'Level and purpose',
	roasted: 'Roast date',
	batch: 'Batch',
	best_from: 'Best from',
	form: 'Whole bean or ground',
	weight: 'Weight',
	notes: 'Tasting notes',
	qr: 'QR code',
	roaster: 'Roaster name'
};
