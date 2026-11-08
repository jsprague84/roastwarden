/**
 * Roast guidance references (docs/roast-guidance.md): the roast levels, a
 * cited and dated reference table, and the colour-meter conversions. Every number here names
 * its sources; "derived" rows are Roastwarden defaults built from the cited points, and their
 * hints say so. Each source was read and graded when the table was made (the review date
 * below); read them again yearly and move the date.
 */

// Reviewed 2026-10-24.

/** a: standards body, instrument maker or measured study; b: recognised expert, educator or
 * vendor documentation; c: community consensus, shop pages, forums. */
type SourceType = 'a' | 'b' | 'c';

export interface Source {
	id: string;
	title: string;
	url: string;
	type: SourceType;
	/** Publication date, as precise as known ('' when undated). */
	date: string;
}

export const SOURCES: Source[] = [
	{
		id: 'agtron2018',
		title: 'Agtron M-Basic II manual (2018)',
		url: 'https://static1.squarespace.com/static/5d77ebfe1fc5e024160b28c5/t/5ee91d0345641c11725cbfb8/1592335620543/m-basic+ii+om++++++++.pdf',
		type: 'a',
		date: '2018-06-15' // table updated 2018-06-15; printed 2020
	},
	{
		id: 'sca2015',
		title: 'SCAA standard "Roast Level for Cupping" (2015)',
		url: 'https://www.coffeelabequipment.com/roast-level-for-cupping-7-13-15_-_remarked.pdf',
		type: 'a',
		date: '2015-07-13'
	},
	{
		id: 'mondry2004',
		title: 'Mondry, "Measuring Roast Color", SCAA 2004 (via Schwartzberg, Fig. 2)',
		url: 'https://library.sweetmarias.com/wp-content/uploads/2020/08/Coffee-Roasting-Schwartzenberg.pdf',
		type: 'a',
		date: '2004'
	},
	{
		id: 'schwartzberg2011',
		title: 'Schwartzberg (UMass), "Coffee Roasting" roast-loss mass balance (2011)',
		url: 'https://library.sweetmarias.com/wp-content/uploads/2020/08/Coffee-Roasting-Schwartzenberg.pdf',
		type: 'a',
		date: '2011'
	},
	{
		id: 'schenker2000',
		title: 'Schenker, ETH Diss. 13620, 100 g fluid bed (2000)',
		url: 'https://library.sweetmarias.com/wp-content/uploads/2020/08/roasting_hot_air_doctorate.pdf',
		type: 'a',
		date: '2000'
	},
	{
		id: 'munchow2020',
		title: 'Münchow, Alstrup, Steen, Giacalone, Beverages 6:29 (2020)',
		// Research read the open-access copy:
		// https://findresearcher.sdu.dk/ws/files/169555397/M_nchow_et_al_2020_Beverages.pdf
		url: 'https://www.mdpi.com/2306-5710/6/2/29',
		type: 'a',
		date: '2020-05-08'
	},
	{
		id: 'alstrup2020',
		title: 'Alstrup et al., Beverages 6:70 (2020)',
		url: 'https://doi.org/10.3390/beverages6040070',
		type: 'a',
		date: '2020-12-03'
	},
	{
		id: 'ico2004',
		title: 'ICO Resolution 420 (2004)',
		url: 'https://ico.org/wp-content/documents/iccres420e.pdf',
		type: 'a',
		date: '2004-05-21'
	},
	{
		id: 'nolasco2022',
		title: 'Nolasco et al., Foods 11:2834 (2022)',
		// The research gives only the DOI (doi:10.3390/foods11182834); this is its resolver URL.
		url: 'https://doi.org/10.3390/foods11182834',
		type: 'a',
		date: '2022'
	},
	{
		id: 'rao2014',
		title: 'Rao, "Roast development time", Daily Coffee News (2014)',
		url: 'https://dailycoffeenews.com/2014/08/12/roast-development-time-and-the-prerequisite-for-any-successful-roast',
		type: 'b',
		date: '2014-08-12'
	},
	{
		id: 'rao2016',
		title: 'Rao, "Development Time Ratio" (2016)',
		url: 'https://www.scottrao.com/blog/2016/8/25/development-time-ratio',
		type: 'b',
		date: '2016-08-25'
	},
	{
		id: 'rao2016b',
		title: 'Rao, "Be careful when interpreting bean data" (2016)',
		url: 'https://www.scottrao.com/blog/be-careful-when-interpreting-bean-data',
		type: 'b',
		date: '2016-09-07'
	},
	{
		id: 'rao2022',
		title: 'Rao, "Maillardmania" (2022)',
		url: 'https://www.scottrao.com/blog/maillardmania',
		type: 'b',
		date: '2022-07-13'
	},
	{
		id: 'rao2023',
		title: 'Rao, "What changed in coffee over the past 30 years" (2023)',
		url: 'https://www.scottrao.com/blog/2023/10/6/what-changed-in-coffee-over-the-past-30-years',
		type: 'b',
		date: '2023-10-06'
	},
	{
		id: 'rao2023b',
		title: 'Rao, "A Comparison of IMF vs Loring" (2023)',
		url: 'https://www.scottrao.com/blog/imfvsloring',
		type: 'b',
		date: '2023-05-18' // time.md and the spec; colour.md says 2023-05-19
	},
	{
		id: 'rao2024',
		title: 'Rao, "Prodigal QC Protocols" (2024)',
		url: 'https://www.scottrao.com/blog/2024/5/7/prodigal-qc-protocols',
		type: 'b',
		date: '2024-05-07' // URL and spec; colour.md says 2024-05-08
	},
	{
		id: 'wendelboe2022',
		title: 'ROEST blog, "Tim\'s roesting tips" (2022)',
		url: 'https://www.roestcoffee.com/blog/tims-roesting-tips',
		type: 'b',
		date: '2022-01-25'
	},
	{
		id: 'hoos2021',
		title: 'Hoos, "A tale of two color meters" (2021)',
		url: 'https://hoos.coffee/blog/a-tale-of-two-color-meters',
		type: 'b',
		date: '2021'
	},
	{
		id: 'hoos2023',
		title: 'Rob Hoos on the IKAWA blog (2023)',
		url: 'https://www.ikawacoffee.com/blog/new-rob-hoos-blog/',
		type: 'b',
		date: '2023-09-15'
	},
	{
		id: 'cooper2019',
		title: 'Cooper & Hoos, "Can you taste the roasting system?", Daily Coffee News (2019)',
		url: 'https://dailycoffeenews.com/2019/05/24/can-you-taste-the-roasting-system-matching-roasting-profiles-on-different-machines',
		type: 'b',
		date: '2019-05-24'
	},
	{
		id: 'luther2019',
		title: 'Artisan blog, "How close" (2019)',
		url: 'https://artisan-roasterscope.blogspot.com/2019/05/how-close.html',
		type: 'b',
		date: '2019-05'
	},
	{
		id: 'luther2023',
		title: 'Artisan blog, "Understanding roast color" (2023)',
		url: 'https://artisan-roasterscope.blogspot.com/2023/03/understanding-roast-color.html',
		type: 'b',
		date: '2023-03-05'
	},
	{
		id: 'sm2023',
		title: 'Sweet Maria\'s, "How to calculate weight loss" (2023)',
		// Research read it via a web.archive.org snapshot (2025-01-17); no archive URL recorded.
		url: 'https://library.sweetmarias.com/how-to-calculate-weight-loss-in-coffee-roasting/',
		type: 'b',
		date: '2023-08-17'
	},
	{
		id: 'kornman2016',
		title: 'Kornman, Royal Coffee, "Green coffee analytics part IV: density" (2016)',
		url: 'https://royalcoffee.com/green-coffee-analytics-part-iv-density/',
		type: 'b',
		date: '2016-10-19'
	},
	{
		id: 'coffeereview2026',
		title: 'Coffee Review roast definitions + 2026 review data',
		// Review data parsed from https://www.coffeereview.com/review/ (Aug–Oct 2026);
		// the definitions page itself is dated 2014-07-12.
		url: 'https://www.coffeereview.com/roast-definitions/',
		type: 'b',
		date: '2026'
	},
	{
		id: 'torque',
		title: 'Torque Coffees, three stages of coffee roasting',
		// colour.md also cites Torque's roast-levels page (Agtron/WL bands):
		// https://torque.coffee/blogs/brew-guides/coffee-roast-levels-explained-torque-coffees
		url: 'https://torque.coffee/blogs/brew-guides/blogs-brew-guides-three-stages-of-coffee-roasting',
		type: 'c',
		date: '' // undated
	},
	{
		id: 'scaexam',
		title: 'SCA sample-roasting problems (Q Grader study guide)',
		url: 'https://open-exam-prep.com/study-guides/sca-q-grader/roasted-grading-sample-roasting/roasting-problems-exam',
		type: 'c',
		date: '' // undated
	}
];

export function sourceById(id: string): Source | undefined {
	return SOURCES.find((s) => s.id === id);
}

// --- Roast levels: the SCA colour tiles on ground Agtron Gourmet (agtron2018) ---

export type LevelKey =
	'extremely_light' | 'very_light' | 'light' | 'medium_light' | 'medium' | 'medium_dark' | 'dark';

export interface Level {
	key: LevelKey;
	label: string;
	/** Ground Agtron (Gourmet) band; each SCA tile is ±5. */
	agtron: [number, number];
}

/** Lightest first. Ground, because the Agtron manual makes the ground reading the roast
 * classification (whole bean only shows evenness). */
export const LEVELS: Level[] = [
	{ key: 'extremely_light', label: 'Extremely light', agtron: [90, 150] },
	{ key: 'very_light', label: 'Very light', agtron: [80, 90] },
	{ key: 'light', label: 'Light', agtron: [70, 80] },
	{ key: 'medium_light', label: 'Medium light', agtron: [60, 70] },
	{ key: 'medium', label: 'Medium', agtron: [50, 60] },
	{ key: 'medium_dark', label: 'Medium dark', agtron: [40, 50] },
	{ key: 'dark', label: 'Dark', agtron: [0, 40] }
];

export function isLevel(k: string | null | undefined): k is LevelKey {
	return LEVELS.some((l) => l.key === k);
}

export function levelLabel(k: string | null | undefined): string {
	return LEVELS.find((l) => l.key === k)?.label ?? '';
}

/** Typical practice, not a standard (docs/roast-guidance.md, "Purpose → suggested level"). */
export const PURPOSE_LEVEL_NOTE =
	'Typical practice, not a standard: light filter ≈ very light (around Agtron 95 ground), light espresso ≈ light (shops quote 75–83), medium filter ≈ medium light to medium (the SCA cupping reference is 63).';

// --- The reference table ---

/** What a profile can expect, with level defaults. Keys match the server's `expectations`. */
export type MetricKey = 'weight_loss_pct' | 'development_s' | 'dtr_pct' | 'color_ground';

export const METRICS: { key: MetricKey; label: string; unit: string }[] = [
	{ key: 'weight_loss_pct', label: 'Weight loss', unit: '%' },
	{ key: 'development_s', label: 'Development time', unit: 's' },
	{ key: 'dtr_pct', label: 'DTR', unit: '%' },
	{ key: 'color_ground', label: 'Ground colour', unit: 'Agtron' }
];

export interface Reference {
	/** A MetricKey, or another measure named in docs/roast-guidance.md. */
	metric: string;
	/** Levels this row applies to; absent = all. */
	levels?: LevelKey[];
	range?: [number, number];
	/** What the numbers rest on, in a sentence. */
	basis: string;
	sources: string[];
	/** Built by Roastwarden from the cited points, not quoted from a source. */
	derived?: boolean;
	/** How Roastwarden uses it: judged; judged loosely (DTR); shown in a hint only; own history only. */
	use: 'judged' | 'loose' | 'hint' | 'own_history';
}

const WL = 'weight_loss_pct';
const DEV = 'development_s';
const DTR = 'dtr_pct';

export const REFERENCES: Reference[] = [
	// Weight loss by level (Mondry's Agtron → loss curve; Sweet Maria's; Rao; Schenker).
	{
		metric: WL,
		levels: ['extremely_light'],
		range: [11, 13],
		basis: 'Mondry: Agtron 95 → 12 % loss; Sweet Maria’s: 10.3 % at first crack, City− 11.5 %.',
		sources: ['mondry2004', 'sm2023'],
		use: 'judged'
	},
	{
		metric: WL,
		levels: ['very_light'],
		range: [12, 14],
		basis: 'Mondry: Agtron 85 → 13 %; Sweet Maria’s City 12.7 %, City+ 13.3 %; Rao: light ≈ 12 %.',
		sources: ['mondry2004', 'sm2023', 'rao2023'],
		use: 'judged'
	},
	{
		metric: WL,
		levels: ['light'],
		range: [14, 16],
		basis: 'Mondry: Agtron 75 → 15 % (interpolated 80 → 14, 70 → 15.5).',
		sources: ['mondry2004'],
		derived: true,
		use: 'judged'
	},
	{
		metric: WL,
		levels: ['medium_light'],
		range: [15, 17],
		basis: 'Mondry: Agtron 65 → 16 %.',
		sources: ['mondry2004'],
		use: 'judged'
	},
	{
		metric: WL,
		levels: ['medium'],
		range: [15, 18],
		basis: 'Mondry: Agtron 55 → 17 %; Schenker, 100 g fluid bed: 15.3–15.8 % at a medium colour.',
		sources: ['mondry2004', 'schenker2000'],
		use: 'judged'
	},
	{
		metric: WL,
		levels: ['medium_dark'],
		range: [18, 20],
		basis: 'Mondry: Agtron 45 → 19 %.',
		sources: ['mondry2004'],
		use: 'judged'
	},
	{
		metric: WL,
		levels: ['dark'],
		range: [20, 22],
		basis: 'Mondry: Agtron 35 → 20 %, 25 → 22 %.',
		sources: ['mondry2004'],
		use: 'judged'
	},
	{
		metric: 'weight_loss_moisture',
		basis:
			'Loss moves about 1 point per point of green moisture away from 10.5 %: roasted moisture and organic loss converge whatever the start.',
		sources: ['schenker2000', 'sm2023'],
		derived: true,
		use: 'judged'
	},
	{
		metric: 'weight_loss_chaff',
		basis:
			'Chaff blown off in a fluid bed is part of the measured loss: about 0.5–1 point (Schwartzberg assumes 1 %; silverskin is 1–2 % of the bean at most).',
		sources: ['schwartzberg2011', 'nolasco2022'],
		use: 'hint'
	},
	{
		metric: 'weight_loss_spread',
		basis: 'Two coffees at the same colour can differ by about 2 points of loss.',
		sources: ['schenker2000'],
		use: 'hint'
	},

	// Development time, FC → DROP, in seconds (judged over DTR on a fluid bed).
	{
		metric: DEV,
		levels: ['extremely_light'],
		range: [30, 75],
		basis: 'Tim Wendelboe: 45–65 s for light roasts on a ROEST (roasts of 6.5 min or less).',
		sources: ['wendelboe2022'],
		derived: true,
		use: 'judged'
	},
	{
		metric: DEV,
		levels: ['very_light'],
		range: [45, 90],
		basis: 'Wendelboe 45–65 s; Alstrup’s “fast” 90 s tasted fruity, sweet and acidic.',
		sources: ['wendelboe2022', 'alstrup2020'],
		derived: true,
		use: 'judged'
	},
	{
		metric: DEV,
		levels: ['light'],
		range: [60, 120],
		basis: 'Alstrup et al.: fast 90 s, medium 143 s at the same colour.',
		sources: ['alstrup2020', 'munchow2020'],
		derived: true,
		use: 'judged'
	},
	{
		metric: DEV,
		levels: ['medium_light'],
		range: [90, 150],
		basis: 'Alstrup et al.: medium 143 s.',
		sources: ['alstrup2020'],
		derived: true,
		use: 'judged'
	},
	{
		metric: DEV,
		levels: ['medium'],
		range: [120, 210],
		basis: 'Alstrup et al.: medium 143 s, slow 266 s.',
		sources: ['alstrup2020'],
		derived: true,
		use: 'judged'
	},
	{
		metric: 'development_effect',
		basis:
			'At equal colour, longer development gives less acidity and sweetness and more bitterness; body is unaffected (80–405 s studied).',
		sources: ['munchow2020'],
		use: 'hint'
	},

	// DTR: shown, judged loosely (Rao coined it for drum roasters at > 60 % load).
	{
		metric: DTR,
		levels: ['extremely_light', 'very_light'],
		range: [10, 17],
		basis:
			'Rao: on air roasters roasting light, most roasts end up with a DTR well below 20 %; Wendelboe’s times imply 12–17 %.',
		sources: ['rao2023', 'wendelboe2022'],
		derived: true,
		use: 'loose'
	},
	{
		metric: DTR,
		levels: ['light'],
		range: [12, 20],
		basis: 'Rao: sample roasters run much lower DTRs; about 15 % with high power for the batch.',
		sources: ['rao2016'],
		derived: true,
		use: 'loose'
	},
	{
		metric: DTR,
		levels: ['medium_light', 'medium'],
		range: [18, 25],
		basis: 'Rao: 20–25 % for drum roasters, light-medium and medium roasts.',
		sources: ['rao2014'],
		use: 'loose'
	},

	// Colour.
	{
		metric: 'color_ground',
		basis: 'The SCA colour tiles on the ground Agtron (Gourmet) scale, each tile ±5.',
		sources: ['agtron2018'],
		use: 'judged'
	},
	{
		metric: 'color_tolerance',
		range: [2, 3],
		basis: 'Batch QC holds a roast within ±2 (Rao) to ±3 (Hoos) points of its own target.',
		sources: ['rao2024', 'hoos2021'],
		use: 'hint'
	},
	{
		metric: 'color_drift',
		basis:
			'Readings drift darker by up to 3 points in 24 h: measure 30 min to 4 h after the roast, at room temperature, and note the grind.',
		sources: ['agtron2018', 'sca2015'],
		use: 'hint'
	},
	{
		metric: 'color_whole_gap',
		basis:
			'Whole minus ground is shown, not judged: there is no fluid-bed data, and grind alone moves a ground reading from 63 to 85.',
		sources: ['coffeereview2026', 'hoos2023', 'luther2023'],
		use: 'hint'
	},

	// Times and temperatures: this roaster's own history only.
	{
		metric: 'total_s',
		basis:
			'Judged against this profile’s own roasts. For context: 6–7 min is common on air roasters (Rao); the SCA sample roast takes 8–12 min.',
		sources: ['rao2023b', 'sca2015'],
		use: 'own_history'
	},
	{
		metric: 'pace',
		basis:
			'Reaching the usual colour in about half or twice the usual time is too fast or too slow.',
		sources: ['scaexam'],
		use: 'hint'
	},
	{
		metric: 'phase_split',
		basis:
			'Drying 30–45 %, Maillard 35–45 %, development 15–25 % is community practice; Rao finds the advice on Maillard length unsupported.',
		sources: ['torque', 'rao2022'],
		use: 'hint'
	},
	{
		metric: 'temperatures',
		basis:
			'Thermocouple readings differ between machines (probe size, placement), so FC, drop and dry-end temperatures are judged only against this roaster’s own roasts.',
		sources: ['cooper2019', 'rao2016b'],
		use: 'own_history'
	},
	{
		metric: 'rmse',
		basis:
			'Tracking is judged against own history; Artisan’s author calls about 1 °C “not too bad”.',
		sources: ['luther2019'],
		use: 'hint'
	},

	// Green coffee.
	{
		metric: 'moisture_green',
		range: [8, 12.5],
		basis:
			'ICO Resolution 420 (ISO 6673): green coffee for export holds 8–12.5 % moisture; typical specialty is about 10.5 %.',
		sources: ['ico2004', 'sm2023'],
		use: 'judged'
	},
	{
		metric: 'density_green',
		range: [640, 690],
		basis:
			'One importer’s classes, not a standard: about 640 g/L is low, 670 average, above 690 dense (free-settled); density falls about 40 g/L per point of moisture.',
		sources: ['kornman2016'],
		use: 'hint'
	}
];

export interface Default {
	range: [number, number];
	basis: string;
	sources: string[];
	derived: boolean;
}

/** The level's expected ranges, or null where no source covers the level. */
export function defaultsFor(level: LevelKey | null | undefined): Record<MetricKey, Default | null> {
	const out: Record<MetricKey, Default | null> = {
		weight_loss_pct: null,
		development_s: null,
		dtr_pct: null,
		color_ground: null
	};
	if (!level) return out;
	for (const key of [WL, DEV, DTR] as MetricKey[]) {
		const r = REFERENCES.find((x) => x.metric === key && x.levels?.includes(level) && x.range);
		if (r?.range)
			out[key] = { range: r.range, basis: r.basis, sources: r.sources, derived: !!r.derived };
	}
	const l = LEVELS.find((x) => x.key === level)!;
	const tiles = REFERENCES.find((x) => x.metric === 'color_ground')!;
	out.color_ground = {
		range: l.agtron,
		basis: `${l.label}: ground Agtron ${bandText(l.agtron)}. ${tiles.basis}`,
		sources: tiles.sources,
		derived: false
	};
	return out;
}

export function bandText([lo, hi]: [number, number]): string {
	if (lo <= 0) return `below ${hi}`;
	if (hi >= 150) return `${lo}+`;
	return `${lo}–${hi}`;
}

// --- Weight loss and moisture ---

/** Typical specialty green moisture, %, the point the loss table assumes. */
export const BASE_MOISTURE = 10.5;

/** Loss range shifted for the green moisture (≈ 1 point per point from 10.5 %). */
export function moistureCorrected(
	range: [number, number],
	moisture: number | null | undefined
): [number, number] {
	if (moisture == null || !Number.isFinite(moisture)) return range;
	const d = moisture - BASE_MOISTURE;
	return [round1(range[0] + d), round1(range[1] + d)];
}

/** Mondry's ground Agtron → weight loss points (via Schwartzberg, Fig. 2). */
const MONDRY: [number, number][] = [
	[95, 12],
	[85, 13],
	[75, 15],
	[65, 16],
	[55, 17],
	[45, 19],
	[35, 20],
	[25, 22]
];

function agtronForLossExact(loss: number): number {
	if (loss <= MONDRY[0][1]) return MONDRY[0][0];
	for (let i = 1; i < MONDRY.length; i++) {
		const [a0, l0] = MONDRY[i - 1];
		const [a1, l1] = MONDRY[i];
		if (loss <= l1) return a0 + ((loss - l0) / (l1 - l0)) * (a1 - a0);
	}
	return MONDRY[MONDRY.length - 1][0];
}

/**
 * The colour a weight loss usually means (the inverse of Mondry's curve), after the moisture
 * correction. `agtron` spans ±1 point of loss, rounded out to 5s, since coffees differ by
 * about 2 points at the same colour. Outside 12–22 % loss the ends clamp.
 */
export function levelForLoss(
	lossPct: number,
	moisture?: number | null
): { level: LevelKey; agtron: [number, number] } {
	const loss =
		lossPct - (moisture != null && Number.isFinite(moisture) ? moisture - BASE_MOISTURE : 0);
	const mid = agtronForLossExact(loss);
	const lo = Math.floor(agtronForLossExact(loss + 1) / 5) * 5;
	const hi = Math.ceil(agtronForLossExact(loss - 1) / 5) * 5;
	return { level: levelForAgtron(mid), agtron: [lo, hi] };
}

export function levelForAgtron(agtron: number): LevelKey {
	for (const l of LEVELS) if (agtron >= l.agtron[0]) return l.key;
	return 'dark';
}

// --- Colour meters ---

/** Tonino → ground Agtron, the maker's table (13-sample fit); cupping 105 ≈ 63. */
const TONINO: [number, number][] = [
	[60, 30],
	[75, 40],
	[85, 50],
	[100, 60],
	[115, 70],
	[125, 80],
	[130, 90]
];

export interface AgtronReading {
	agtron: number | null;
	/** Converted or read as Gourmet-like: compare against your own readings first. */
	approximate: boolean;
	note: string;
}

/** A meter reading on `scale` as ground Agtron (Gourmet), when there is a way to say. */
export function toAgtron(
	value: number | null | undefined,
	scale: string | null | undefined
): AgtronReading {
	if (value == null || !Number.isFinite(value))
		return { agtron: null, approximate: false, note: '' };
	switch (scale ?? 'agtron') {
		case 'agtron':
			return { agtron: value, approximate: false, note: '' };
		case 'tonino': {
			const first = TONINO[0];
			const last = TONINO[TONINO.length - 1];
			let agtron: number;
			if (value <= first[0]) agtron = first[1];
			else if (value >= last[0]) agtron = last[1];
			else {
				const i = TONINO.findIndex(([t]) => t >= value);
				const [t0, a0] = TONINO[i - 1];
				const [t1, a1] = TONINO[i];
				agtron = a0 + ((value - t0) / (t1 - t0)) * (a1 - a0);
			}
			return {
				agtron: round1(agtron),
				approximate: true,
				note: 'Approximate (the maker’s 13-sample fit); a Tonino can’t resolve above about Agtron 90.'
			};
		}
		case 'lebrew':
			return {
				agtron: value,
				approximate: true,
				note: 'Read as Gourmet-like: there is no published conversion to an Agtron M-Basic; compare against your own readings first.'
			};
		default:
			return {
				agtron: null,
				approximate: false,
				note: 'No published band for this meter; only your own readings compare.'
			};
	}
}

function round1(x: number): number {
	return Math.round(x * 10) / 10;
}

/** The basis sentence of a hint-only reference row (chaff, DTR caveat, …). */
export function refBasis(metric: string): string {
	return REFERENCES.find((r) => r.metric === metric)?.basis ?? '';
}

/** Green moisture against ICO Resolution 420 (8–12.5 %), as an expectation for RangeBand. */
export function greenMoistureRange(): {
	range: [number, number];
	source: 'default';
	basis: string;
	sources: string[];
} {
	const r = REFERENCES.find((x) => x.metric === 'moisture_green')!;
	return { range: r.range!, source: 'default', basis: r.basis, sources: r.sources };
}

/** One importer's density classes, not a standard: a word for the hint, never amber. */
export function densityClass(gPerL: number): 'low' | 'average' | 'dense' {
	return gPerL < 655 ? 'low' : gPerL < 690 ? 'average' : 'dense';
}
