/**
 * The label studio (docs/labels.md, "Label studio"): designs, assets, brands, printers, print runs
 * and sheets.
 */

import { BASE_URL, downloadFile, errorMessage, request } from '#lib/api/http.js';
import type { RoastedMove } from '#lib/api/roasted.js';

export type DesignItem = import('#lib/labels/labels.js').LabelDesign;

interface DesignVersion {
	version: number;
	note?: string | null;
	created_by: string;
	created_at: string;
}

export interface SavedDesign {
	id: string;
	kind: 'saved' | 'folder' | 'builtin';
	name: string;
	description?: string | null;
	version: number;
	shown: number;
	source: string;
	manifest?: unknown;
	versions: DesignVersion[];
}

export interface Asset {
	name: string;
	kind: 'image' | 'font';
	content_type: string;
	size: number;
	family?: string | null;
	note?: string | null;
	created_by: string;
	created_at: string;
}

export interface Brand {
	id: string;
	name: string;
	tagline?: string | null;
	place?: string | null;
	website?: string | null;
	logo?: string | null;
	ink?: string | null;
	accent?: string | null;
	paper?: string | null;
	font_display?: string | null;
	font_body?: string | null;
	font_script?: string | null;
}

/** A label printer (docs/labels.md, "Printers") */
export interface LabelPrinter {
	id: string;
	name: string;
	url: string;
	is_default: boolean;
	tray?: string | null;
	color?: string | null;
	/** auto, pdf, pwg, urf */
	send: string;
	fingerprint?: string | null;
	/** What it said when last asked */
	info?: {
		make_and_model?: string;
		state?: string;
		state_reasons?: string[];
		pdf?: boolean;
		color?: boolean;
		labels_media?: boolean;
		media_sources?: string[];
		formats?: string[];
		trays?: { source?: string; width?: number; height?: number; media_type?: string }[];
	} | null;
	info_at?: string | null;
	calibrations: {
		sheet: string;
		x_mm: number;
		y_mm: number;
		tray?: string | null;
		updated_at: string;
	}[];
	problem?: string;
	/** pdf, or pwg/urf with its resolution */
	sends?: string;
}

export interface FoundPrinter {
	name: string;
	url: string;
	model?: string;
	uuid?: string;
	tls: boolean;
	color: boolean;
}

/** A print run as listed (docs/labels.md, "Print runs") */
export interface LabelRunSummary {
	id: string;
	created_at: string;
	created_by: string;
	sheet: string;
	sheet_name: string;
	start: number;
	labels: number;
	pages: number;
	/** "Guji (#7) ×2, Palmichal (#5)" */
	summary: string;
	/** made, sent, pending, processing, stopped, completed, aborted, canceled, failed, printed */
	state: string;
	printer?: string;
	job_id?: number;
	reprint_of?: string;
	note?: string;
}

interface LabelRunItem {
	roast?: string;
	name: string;
	design: string;
	count: number;
	params?: Record<string, unknown>;
	brand?: string;
	data: Record<string, unknown>;
}

export interface LabelRun extends LabelRunSummary {
	items: LabelRunItem[];
	states: { at: string; state: string; reasons?: string[]; sheets?: number }[];
	tray?: string;
	/** A make with the same labels as a run not yet printed answered that run */
	reused?: boolean;
	/** The bags it took off its roasts (an item's `pack`) */
	packed?: RoastedMove[];
}

export interface RunPrinted {
	message: string;
	printer: string;
	job_id?: number;
	job_state?: string;
	run: string;
	/** application/pdf, image/pwg-raster or image/urf */
	sent_as?: string;
	approval?: { url: string };
}

export interface SheetNext {
	sheet: string;
	name: string;
	per_sheet: number;
	next_label: number;
	run_id?: string;
	updated_at?: string;
}

/** A design's save: its id and version, and the check's report on sample data */
export interface DesignSaved {
	design: SavedDesign;
	report?: LabelCheck['report'];
}

export interface BeanLabel {
	design?: string | null;
	params?: Record<string, unknown>;
	brand?: string | null;
}

export interface LabelCheck {
	ok: boolean;
	errors?: {
		file?: string;
		line?: number;
		column?: number;
		from?: number;
		to?: number;
		message: string;
		hints?: string[];
	}[];
	report?: {
		ok: boolean;
		w: number;
		h: number;
		round?: boolean;
		issues: { kind: string; message: string }[];
		lines: { text: string; font: string; pt: number; x: number; y: number; w: number; h: number }[];
		images?: { kind: string; x: number; y: number; w: number; h: number; module?: number }[];
		fields: { shown: string[]; missing: string[] };
		manifest?: unknown;
		params?: Record<string, unknown>;
		notes?: unknown[];
	};
	/** base64 PNG */
	png?: string;
	design: string;
	sheet: string;
	/** With `retail`: what a bag that is sold needs */
	retail?: {
		ok: boolean;
		bag: string;
		min_net_mm: number;
		issues: { kind: string; message: string }[];
		passed: string[];
		cannot_check: string[];
	};
}

export const labelStudio = {
	designs: () => request<DesignItem[]>('/api/label-designs'),
	design: (ref: string) => request<SavedDesign>(`/api/label-designs/${encodeURIComponent(ref)}`),
	check: (body: Record<string, unknown>, signal?: AbortSignal) =>
		request<LabelCheck>(
			'/api/labels/check',
			{ method: 'POST', body: JSON.stringify(body), signal },
			true
		),
	/** A sheet of labels from a design's source or a saved design, as SVG (the first page,
	 *  outlined) or PDF; nothing is saved */
	renderSvg: async (body: Record<string, unknown>, signal?: AbortSignal): Promise<string> => {
		const res = await fetch(`${BASE_URL}/api/labels/render`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ ...body, format: 'svg' }),
			signal
		});
		if (!res.ok)
			throw new Error(
				errorMessage(await res.text().catch(() => '')) || `Render failed: ${res.status}`
			);
		return res.text();
	},
	rollback: (id: string, version: number) =>
		request<unknown>(
			`/api/label-designs/${encodeURIComponent(id)}/rollback`,
			{
				method: 'POST',
				body: JSON.stringify({ version })
			},
			true
		),
	/** Save a new design (version 1); it must compile. Answers the check's report. */
	create: (body: {
		id: string;
		name?: string;
		description?: string;
		source: string;
		note?: string;
		sheet?: string;
	}) =>
		request<DesignSaved>(
			'/api/label-designs',
			{ method: 'POST', body: JSON.stringify(body) },
			true
		),
	/** A new version from the source (checked first), or a new name or description */
	edit: (
		id: string,
		body: { name?: string; description?: string; source?: string; note?: string; sheet?: string }
	) =>
		request<DesignSaved>(
			`/api/label-designs/${encodeURIComponent(id)}`,
			{ method: 'PUT', body: JSON.stringify(body) },
			true
		),
	remove: (id: string) =>
		request<void>(`/api/label-designs/${encodeURIComponent(id)}`, { method: 'DELETE' }, true),
	assets: () => request<Asset[]>('/api/label-assets'),
	assetUrl: (name: string) => `${BASE_URL}/api/label-assets/${encodeURIComponent(name)}`,
	/** Add or replace an image or a font: the file is the body */
	uploadAsset: async (name: string, file: Blob, note?: string): Promise<Asset> => {
		const q = note?.trim() ? `?note=${encodeURIComponent(note.trim())}` : '';
		const res = await fetch(`${BASE_URL}/api/label-assets/${encodeURIComponent(name)}${q}`, {
			method: 'PUT',
			headers: { 'Content-Type': file.type || 'application/octet-stream' },
			body: file
		});
		if (res.status === 401) {
			window.dispatchEvent(new CustomEvent('auth_required'));
			throw new Error('Authentication required');
		}
		if (!res.ok) throw new Error(errorMessage(await res.text()));
		return res.json();
	},
	removeAsset: (name: string) =>
		request<void>(`/api/label-assets/${encodeURIComponent(name)}`, { method: 'DELETE' }, true),
	brands: () => request<Brand[]>('/api/label-brands'),
	putBrand: (id: string, b: Partial<Brand>) =>
		request<Brand>(
			`/api/label-brands/${encodeURIComponent(id)}`,
			{ method: 'PUT', body: JSON.stringify(b) },
			true
		),
	removeBrand: (id: string) =>
		request<void>(`/api/label-brands/${encodeURIComponent(id)}`, { method: 'DELETE' }, true),
	/** A printer asked now (default the default printer) */
	printer: (id?: string) =>
		request<{
			configured: boolean;
			problem?: string;
			printer?: { make_and_model?: string; state?: string; media_sources?: string[] };
		}>(`/api/labels/printer${id ? `?printer=${encodeURIComponent(id)}` : ''}`),
	// Print runs (docs/labels.md, "Print runs")
	runs: (limit = 30) => request<LabelRunSummary[]>(`/api/label-runs?limit=${limit}`),
	run: (id: string) => request<LabelRun>(`/api/label-runs/${encodeURIComponent(id)}`),
	makeRun: (body: Record<string, unknown>) =>
		request<LabelRun>('/api/label-runs', { method: 'POST', body: JSON.stringify(body) }, true),
	/** The run's first sheet as SVG, outlined, before it is made */
	previewRun: async (body: Record<string, unknown>, signal?: AbortSignal): Promise<string> => {
		const res = await fetch(`${BASE_URL}/api/label-runs/preview`, {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(body),
			signal
		});
		if (!res.ok)
			throw new Error(
				errorMessage(await res.text().catch(() => '')) || `Preview failed: ${res.status}`
			);
		return res.text();
	},
	runPdf: (id: string) =>
		downloadFile(
			`/api/label-runs/${encodeURIComponent(id)}/pdf`,
			`labels-run-${id.slice(0, 8)}.pdf`
		),
	printRun: (id: string, how: { printer?: string; tray?: string; color?: string } = {}) =>
		request<RunPrinted>(
			`/api/label-runs/${encodeURIComponent(id)}/print`,
			{ method: 'POST', body: JSON.stringify(how) },
			true
		),
	runDone: (id: string) =>
		request<LabelRun>(`/api/label-runs/${encodeURIComponent(id)}/done`, { method: 'POST' }, true),
	reprint: (id: string, body: { start?: number; labels?: number[]; note?: string }) =>
		request<LabelRun>(
			`/api/label-runs/${encodeURIComponent(id)}/reprint`,
			{ method: 'POST', body: JSON.stringify(body) },
			true
		),
	sheets: () => request<SheetNext[]>('/api/label-sheets'),
	// Printers (docs/labels.md, "Printers")
	printers: () => request<LabelPrinter[]>('/api/label-printers'),
	/** One printer, asked now how it is */
	printerNow: (id: string) =>
		request<LabelPrinter>(`/api/label-printers/${encodeURIComponent(id)}`),
	addPrinter: (p: Partial<LabelPrinter>) =>
		request<LabelPrinter>('/api/label-printers', { method: 'POST', body: JSON.stringify(p) }, true),
	editPrinter: (id: string, p: Partial<LabelPrinter>) =>
		request<LabelPrinter>(
			`/api/label-printers/${encodeURIComponent(id)}`,
			{ method: 'PUT', body: JSON.stringify(p) },
			true
		),
	removePrinter: (id: string) =>
		request<void>(`/api/label-printers/${encodeURIComponent(id)}`, { method: 'DELETE' }, true),
	defaultPrinter: (id: string) =>
		request<LabelPrinter>(
			`/api/label-printers/${encodeURIComponent(id)}/default`,
			{ method: 'POST' },
			true
		),
	findPrinters: () =>
		request<{ printers: FoundPrinter[]; note: string }>('/api/label-printers/find'),
	certificate: (url: string) =>
		request<{ url: string; fingerprint: string }>(
			'/api/label-printers/certificate',
			{ method: 'POST', body: JSON.stringify({ url }) },
			true
		),
	calibrate: (
		id: string,
		sheet: string,
		body: {
			x_mm?: number;
			y_mm?: number;
			first?: [number, number];
			last?: [number, number];
			tray?: string;
		}
	) =>
		request<LabelPrinter>(
			`/api/label-printers/${encodeURIComponent(id)}/calibration/${encodeURIComponent(sheet)}`,
			{ method: 'PUT', body: JSON.stringify(body) },
			true
		),
	clearCalibration: (id: string, sheet: string) =>
		request<void>(
			`/api/label-printers/${encodeURIComponent(id)}/calibration/${encodeURIComponent(sheet)}`,
			{ method: 'DELETE' },
			true
		),
	calibrationPdf: (sheet: string, printer?: string) =>
		downloadFile(
			`/api/labels/calibration.pdf?sheet=${encodeURIComponent(sheet)}${printer ? `&printer=${encodeURIComponent(printer)}` : ''}`,
			`calibration-${sheet}${printer ? `-${printer}` : ''}.pdf`
		),
	setNext: (sheet: string, next: number) =>
		request<SheetNext>(
			`/api/label-sheets/${encodeURIComponent(sheet)}`,
			{ method: 'PUT', body: JSON.stringify({ next }) },
			true
		),
	setBeanLabel: (beanId: string, l: BeanLabel) =>
		request<BeanLabel>(
			`/api/beans/${encodeURIComponent(beanId)}/label`,
			{
				method: 'PUT',
				body: JSON.stringify(l)
			},
			true
		)
};
