/**
 * The charts' neutral colours, light and dark, with no store: the dashboard reads them through
 * chartTheme.ts (the theme store), roastwarden.com's roast demo directly (src/demo).
 */

/** Neutral colours for ECharts, which cannot read CSS variables. */
export interface ChartPalette {
	/** Axis tick labels, legend text, axis names. */
	axisText: string;
	/** Grid / split lines and tooltip borders. */
	line: string;
	/** Fainter split lines (secondary axes, radar rings). */
	lineFaint: string;
	/** Placeholder titles ("Waiting for telemetry"), dashed helper lines. */
	mutedText: string;
	/** Mark-line labels (landmarks). */
	labelText: string;
	tooltipBg: string;
	tooltipText: string;
	/** Draggable point handles in the profile designer. */
	handleStroke: string;
	handleStrokeSoft: string;
	/** Font family for canvas text (the scope's labels). */
	font: string;
}

export const CHART_FONT = "'Atkinson Hyperlegible Next Variable', system-ui, sans-serif";

/** The "Chart paper" look (src/app.css). */
const DARK_PALETTE: ChartPalette = {
	axisText: '#a0a9b4',
	line: 'rgba(157, 176, 238, 0.13)',
	lineFaint: 'rgba(157, 176, 238, 0.07)',
	mutedText: '#7b8592',
	labelText: '#d5dae2',
	tooltipBg: 'rgba(34, 39, 47, 0.97)',
	tooltipText: '#e8ebef',
	handleStroke: '#e8ebef',
	handleStrokeSoft: 'rgba(232, 235, 239, 0.6)',
	font: CHART_FONT
};

/** The light grid is the green of recorder chart paper. */
const LIGHT_PALETTE: ChartPalette = {
	axisText: '#525d57',
	line: '#c6dac8',
	lineFaint: '#dde9de',
	mutedText: '#6b766f',
	labelText: '#2c342f',
	tooltipBg: 'rgba(255, 255, 255, 0.98)',
	tooltipText: '#191e1b',
	handleStroke: '#191e1b',
	handleStrokeSoft: 'rgba(25, 30, 27, 0.55)',
	font: CHART_FONT
};

export function paletteFor(light: boolean): ChartPalette {
	return light ? LIGHT_PALETTE : DARK_PALETTE;
}
