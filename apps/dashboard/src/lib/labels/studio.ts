/** Label Studio (docs/labels.md): logic for the design gallery and the design page. */

/** Gallery previews (data URLs, '' when the design would not render), kept for the tab's
 *  life so a trip to a design and back does not lay every design out again. Keyed
 *  `design@version:sheet`. */
export const previewCache: Record<string, string> = {};

/** A saved design's id: letters, digits, `-` and `_` (the server's rule). */
export function validId(id: string): boolean {
	return /^[A-Za-z0-9_-]{1,64}$/.test(id);
}

/** An id made from a name: "Sam's gift" → "sams-gift". */
export function idFrom(name: string): string {
	return name
		.normalize('NFKD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase()
		.replace(/['’]/g, '')
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '')
		.slice(0, 64);
}

/** An asset's name from a file's: letters, digits, '.', '-' and '_', starting with a letter
 *  or digit, its ending kept ("My Logo (2).SVG" → "my-logo-2.svg"). */
export function assetName(file: string): string {
	const dot = file.lastIndexOf('.');
	const ext = dot > 0 ? file.slice(dot + 1).toLowerCase() : '';
	const stem = (dot > 0 ? file.slice(0, dot) : file)
		.normalize('NFKD')
		.replace(/[̀-ͯ]/g, '')
		.replace(/[^A-Za-z0-9_-]+/g, '-')
		.replace(/^[-_]+|-+$/g, '')
		.slice(0, 60 - ext.length);
	return `${stem || 'asset'}${ext ? `.${ext}` : ''}`;
}

/** The Typst a design writes to use an asset. */
export function typstFor(a: { name: string; kind: string; family?: string | null }): string {
	return a.kind === 'font'
		? `text(font: "${a.family ?? a.name}")[…]`
		: `image("${a.name}", width: 0.6in)`;
}

/** A new design's first source: the contract with a manifest, in the dashboard's typeface
 *  (the skill's label-design.md has the same start). */
export const TEMPLATE = `// A label design (docs/labels.md, "Designs"): one Typst file that draws one label.
#import "roastwarden.typ": fit, clamp, join, with

#let design = (
  name: "My label",
  description: "What it is for",
  params: (
    title: (default: "", help: "a line instead of the coffee's name"),
  ),
)

#let label(l, w, h, round) = {
  set text(font: "Atkinson Hyperlegible Next", size: 8.5pt, top-edge: "ascender", bottom-edge: "descender")
  set par(leading: 0.15em)
  let pad = 0.15in
  let inner = if round { w * 0.7 } else { w - 2 * pad }
  let name = if l.params.title != "" { l.params.title } else { l.name }
  box(width: w, height: h, inset: if round { 0pt } else { pad },
    align(if round { center + horizon } else { left + horizon }, stack(dir: ttb, spacing: 3pt, ..(
      fit(name, inner, max: 16pt, min: 8pt, weight: "bold"),
      if join((l.origin, l.process)) != none { clamp(join((l.origin, l.process)), inner, lines: 1) },
      if join((l.level, l.purpose)) != none { text(weight: "bold", upper(join((l.level, l.purpose)))) },
      with("Roasted ", l.roasted),
      if l.message != none { fit(l.message, inner, max: 10pt, style: "italic") },
    ).filter(x => x != none))))
}
`;

/** A check's errors that belong to the design, for the editor's markers: their spans
 *  (UTF-16 offsets, as the editor counts), else their line and column. */
export function problemsOf(
	errors:
		| {
				file?: string;
				line?: number;
				column?: number;
				from?: number;
				to?: number;
				message: string;
				hints?: string[];
		  }[]
		| undefined
): {
	from?: number;
	to?: number;
	line?: number;
	column?: number;
	message: string;
	severity: 'error';
}[] {
	return (errors ?? [])
		.filter(
			(e) => (e.file === 'design.typ' || e.file === undefined) && (e.line || e.from !== undefined)
		)
		.map((e) => ({
			from: e.from,
			to: e.to,
			line: e.line,
			column: e.column,
			message: e.hints?.length ? `${e.message} (hint: ${e.hints.join('; ')})` : e.message,
			severity: 'error' as const
		}));
}

/** Whether the editor's completion pops up by itself with this text before the cursor (on
 *  its line): right after `#`, or in a name (not a number like `12pt`). Anywhere else (after
 *  a closed string or paren, a comma, a space) it stays shut, so Enter is a new line;
 *  Ctrl+Space still asks for it anywhere. */
export function completesHere(before: string): boolean {
	return (
		before.endsWith('#') ||
		/(?<![\p{ID_Continue}_-])[\p{ID_Start}_][\p{ID_Continue}_-]*$/u.test(before)
	);
}
