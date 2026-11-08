/**
 * Reports the server makes with Typst (docs/reports.md): what the Reports page asks for and
 * how it names things. The server does the arithmetic and the layout; this only builds the
 * query and labels the choices.
 */

export type ReportFormat = 'pdf' | 'svg' | 'png' | 'json';
export type Paper = '' | 'letter' | 'a4';
export type Units = '' | 'C' | 'F';

export interface ReportChoices {
	/** '' leaves it to the server's setting (report_paper) */
	paper: Paper;
	/** '' leaves it to the server's setting (report_units) */
	units: Units;
}

/** Minutes east of UTC in this browser, so the report's dates are the reader's. */
export function tzMinutes(d: Date = new Date()): number {
	return -d.getTimezoneOffset();
}

/** `format=svg&tz=-300&paper=a4&units=F`: only what was chosen. */
export function reportQuery(
	format: ReportFormat,
	c: ReportChoices,
	tz: number = tzMinutes()
): string {
	const q = new URLSearchParams({ format, tz: String(tz) });
	if (c.paper) q.set('paper', c.paper);
	if (c.units) q.set('units', c.units);
	return q.toString();
}

/** Where a kind's report is, for its subject: `/api/reports/roast/<id>`. */
export function reportPath(kind: string, subject: string): string {
	return `/api/reports/${encodeURIComponent(kind)}/${encodeURIComponent(subject)}`;
}

export interface RoastLike {
	id: string;
	name: string;
	batch_number?: number | null;
	start_time?: string | null;
	created_at: string;
	status: string;
}

/** `#5 · Commissioning test #6 · 25 Oct 2026` */
export function roastLabel(s: RoastLike, locale?: string): string {
	const when = new Date(s.start_time ?? s.created_at).toLocaleDateString(locale, {
		day: 'numeric',
		month: 'short',
		year: 'numeric'
	});
	return [s.batch_number != null ? `#${s.batch_number}` : null, s.name, when]
		.filter(Boolean)
		.join(' · ');
}

/** Roasts a report can be made of: finished ones, newest first. */
export function reportableRoasts<T extends RoastLike>(list: T[]): T[] {
	return list
		.filter((s) => s.status === 'completed')
		.sort(
			(a, b) => Date.parse(b.start_time ?? b.created_at) - Date.parse(a.start_time ?? a.created_at)
		);
}

/** The file a download gets: `roast-report-batch-5.pdf`. */
export function reportFileName(
	kind: string,
	s: { id: string; batch_number?: number | null }
): string {
	return `${kind}-report-${s.batch_number != null ? `batch-${s.batch_number}` : s.id.slice(0, 8)}.pdf`;
}

export interface CuppingChoices {
	/** A blank form to score on, or the scores */
	mode: 'blank' | 'scores';
	/** Roast ids, in the order picked */
	roasts: string[];
	/** On a blank form: samples of the owner's own as well */
	samples: number;
	/** On a blank form: 2 a page with scales, 6 with boxes */
	perPage: 2 | 6;
	/** The scores of the cuppings on this day (YYYY-MM-DD); '' for the latest */
	date: string;
}

/** The cupping sheet's own query: `blank=true&roasts=a,b&samples=1&per_page=6`. */
export function cuppingParams(c: CuppingChoices): string {
	const q = new URLSearchParams();
	if (c.roasts.length) q.set('roasts', c.roasts.join(','));
	if (c.mode === 'blank') {
		q.set('blank', 'true');
		if (c.samples > 0) q.set('samples', String(Math.min(24, Math.floor(c.samples))));
		if (c.perPage === 6) q.set('per_page', '6');
	} else if (!c.roasts.length && c.date) {
		q.set('date', c.date);
	}
	return q.toString();
}

/** `green-inventory-2026-10-26.pdf`: a dated file name, this browser's day */
export function datedFileName(prefix: string, d: Date = new Date()): string {
	const day = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
	return `${prefix}-${day}.pdf`;
}

/** `cupping-sheet-2026-10-26.pdf`, `cupping-scores-2026-10-26.pdf` */
export function cuppingFileName(mode: CuppingChoices['mode'], d: Date = new Date()): string {
	return datedFileName(`cupping-${mode === 'blank' ? 'sheet' : 'scores'}`, d);
}

/** A coffee's or profile's report starts on the newest roast's (roasts newest first) when it
 *  is one of the choices, else the first choice; '' when there is none. */
export function newestOf(
	roasts: { bean_id?: string | null; profile_id?: string | null }[],
	key: 'bean_id' | 'profile_id',
	choices: { id: string }[]
): string {
	const ids = new Set(choices.map((c) => c.id));
	for (const r of roasts) {
		const v = r[key];
		if (v && ids.has(v)) return v;
	}
	return choices[0]?.id ?? '';
}

/** A report's error in the page's words: the API names its query (`blank`), the page has a
 *  button for it. */
export function reportError(kind: string, message: string): string {
	if (kind !== 'cupping') return message;
	if (/^no roast is cupped yet/i.test(message))
		return 'No roast is cupped yet: “A form to score on” makes a sheet to score on.';
	if (/^none of these roasts is cupped yet/i.test(message))
		return 'None of the ticked roasts is cupped yet: untick them for the latest cupping session, or choose “A form to score on”.';
	return message.replace(/`blank`/g, '“A form to score on”');
}

/** Reports about the whole stock or list: no subject, an address of their own */
export const LIST_KINDS: Record<string, string> = {
	cupping: '/api/reports/cupping',
	inventory: '/api/reports/inventory',
	shopping: '/api/reports/shopping',
	period: '/api/reports/period',
	purchases: '/api/reports/purchases',
	plan: '/api/reports/plan'
};

export type Span = 'this-month' | 'last-month' | 'this-year' | 'last-year' | 'range';

/** A period's query: `month=2026-10`, `year=2025`, or `from=…&to=…` for a range. */
export function spanParams(span: Span, today: Date, from = '', to = ''): string {
	const y = today.getFullYear();
	const m = today.getMonth() + 1;
	const pad = (n: number) => String(n).padStart(2, '0');
	switch (span) {
		case 'this-month':
			return `month=${y}-${pad(m)}`;
		case 'last-month':
			return m === 1 ? `month=${y - 1}-12` : `month=${y}-${pad(m - 1)}`;
		case 'this-year':
			return `year=${y}`;
		case 'last-year':
			return `year=${y - 1}`;
		case 'range': {
			const q = new URLSearchParams();
			if (from) q.set('from', from);
			if (to) q.set('to', to);
			return q.toString();
		}
	}
}

/** A trace's query (docs/reports.md, "Traceability"): a roast back to its green lot, or a
 *  coffee to every bag; '' when there is nothing to trace. */
export function traceParams(of: 'roast' | 'bean', roastId: string, beanId: string): string {
	const id = of === 'roast' ? roastId : beanId;
	return id ? new URLSearchParams({ [of]: id }).toString() : '';
}

/** Kinds that print temperatures, so the unit matters */
export function hasTemperatures(kind: string): boolean {
	return (
		kind === 'roast' ||
		kind === 'compare' ||
		kind === 'coffee' ||
		kind === 'profile' ||
		kind === 'plan'
	);
}

/** The two roasts a compare report starts on (roasts finished, newest first): B the one asked
 *  for (else the newest), A the roast before it on the same roaster (else the next one). A
 *  roast that is not among them is left as asked; '' when there is none. */
export function comparePair(
	roasts: { id: string; device_id: string }[],
	a?: string | null,
	b?: string | null
): { a: string; b: string } {
	const bId = b || roasts[0]?.id || '';
	if (a) return { a, b: bId };
	const at = roasts.findIndex((r) => r.id === bId);
	const after = at >= 0 ? roasts.slice(at + 1) : roasts.filter((r) => r.id !== bId);
	const device = roasts[at]?.device_id;
	const prev = after.find((r) => r.device_id === device) ?? after[0];
	return { a: prev?.id ?? '', b: bId };
}

/** The compare report's own query: `roasts=<A>,<B>` */
export function compareParams(a: string, b: string): string {
	return new URLSearchParams({ roasts: `${a},${b}` }).toString();
}

/** `compare-12-13.pdf`: the batch numbers, else the ids' first characters */
export function compareFileName(
	a?: { id: string; batch_number?: number | null },
	b?: { id: string; batch_number?: number | null }
): string {
	const part = (s?: { id: string; batch_number?: number | null }) =>
		s ? (s.batch_number != null ? String(s.batch_number) : s.id.slice(0, 8)) : 'x';
	return `compare-${part(a)}-${part(b)}.pdf`;
}
