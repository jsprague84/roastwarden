import { isLight } from '#lib/stores/theme.svelte.js';
import { paletteFor, type ChartPalette } from './chartPalettes.js';

export { CHART_FONT, type ChartPalette } from './chartPalettes.js';

/** Reactive: reads the theme store, so a `$derived` chart option re-renders on toggle. */
export function chartPalette(): ChartPalette {
	return paletteFor(isLight());
}
