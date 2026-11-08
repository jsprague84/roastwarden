/**
 * Rows for chart labels that sit side by side on one axis (the History chart's landmark
 * labels): each label takes the lowest row where it clears the last label already in that
 * row, so "FC Start", "FC End" and "Drop" stack instead of overlapping on a phone.
 *
 * `xs` are positions in axis units, `widths` each label's width in pixels, `pxPerUnit`
 * the axis scale. Returns one row index per label, in the input order.
 */
export function labelRows(xs: number[], widths: number[], pxPerUnit: number, gapPx = 4): number[] {
	const order = xs.map((_, i) => i).sort((a, b) => xs[a] - xs[b]);
	const rowEnds: number[] = []; // right edge (px) of the last label in each row
	const rows = new Array<number>(xs.length).fill(0);
	for (const i of order) {
		const centre = xs[i] * pxPerUnit;
		const left = centre - widths[i] / 2;
		let row = rowEnds.findIndex((end) => left >= end + gapPx);
		if (row < 0) {
			row = rowEnds.length;
			rowEnds.push(0);
		}
		rowEnds[row] = centre + widths[i] / 2;
		rows[i] = row;
	}
	return rows;
}

/** Rough width in pixels of a label at `fontSize`, enough to decide overlaps. */
export const labelWidth = (text: string, fontSize = 10): number => text.length * fontSize * 0.6 + 4;
