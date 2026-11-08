/** A design's parameters (docs/labels.md, "Designs"): the form the Labels page shows for
 *  them, from the manifest a check reads (`design.params`), and what goes in the request. */

type ParamKind = 'text' | 'lines' | 'number' | 'bool' | 'choice' | 'color';

export type ParamSpec = {
	key: string;
	kind: ParamKind;
	default: unknown;
	choices?: string[];
	help?: string;
};

function isSpec(v: unknown): v is { default?: unknown; choices?: unknown; help?: unknown } {
	return typeof v === 'object' && v !== null && !Array.isArray(v) && 'default' in v;
}

/** One field per parameter, in the manifest's order. A parameter is its default, or a
 *  dictionary with `default` (and `choices`, `help`). */
export function paramSpecs(manifest: unknown): ParamSpec[] {
	const params = (manifest as { params?: unknown } | null | undefined)?.params;
	if (typeof params !== 'object' || params === null || Array.isArray(params)) return [];
	return Object.entries(params as Record<string, unknown>).map(([key, raw]) => {
		const spec = isSpec(raw) ? raw : { default: raw };
		const def = spec.default ?? '';
		const choices = Array.isArray(spec.choices) ? spec.choices.map(String) : undefined;
		const help = typeof spec.help === 'string' ? spec.help : undefined;
		let kind: ParamKind = 'text';
		if (choices?.length) kind = 'choice';
		else if (typeof def === 'boolean') kind = 'bool';
		else if (typeof def === 'number') kind = 'number';
		else if (typeof def === 'string' && /^#[0-9a-f]{6}$/i.test(def)) kind = 'color';
		else if (typeof def === 'string' && (def.includes('\n') || /ascii|art|lines/i.test(help ?? '')))
			kind = 'lines';
		return { key, kind, default: def, choices, help };
	});
}

/** The values that differ from their defaults: what the request carries. A choice keeps any
 *  text (a design may take its own lines where it lists presets). */
export function changedParams(
	specs: ParamSpec[],
	values: Record<string, unknown>
): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	for (const s of specs) {
		const v = values[s.key];
		if (v === undefined || v === null || v === s.default) continue;
		if (s.kind === 'number') {
			const n = Number(v);
			if (Number.isFinite(n) && n !== s.default) out[s.key] = n;
		} else if (s.kind === 'bool') {
			if (Boolean(v) !== s.default) out[s.key] = Boolean(v);
		} else if (String(v) !== String(s.default)) {
			out[s.key] = String(v);
		}
	}
	return out;
}

/** The form's starting values: the defaults, with what was given (a coffee's own label). */
export function startValues(
	specs: ParamSpec[],
	given: Record<string, unknown> = {}
): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	for (const s of specs) out[s.key] = s.key in given ? given[s.key] : s.default;
	return out;
}

/** What the request carries when the design starts from `base` (the coffee's own label,
 *  which the server lays under the request): the values that differ from the defaults, and
 *  those put back to a default where `base` had another value. */
export function paramsFrom(
	specs: ParamSpec[],
	values: Record<string, unknown>,
	base: Record<string, unknown> = {}
): Record<string, unknown> {
	const out = changedParams(specs, values);
	for (const s of specs) {
		if (s.key in out || !(s.key in base)) continue;
		const v = values[s.key];
		if (v !== undefined && String(v) !== String(base[s.key]))
			out[s.key] = s.kind === 'number' ? Number(v) : s.kind === 'bool' ? Boolean(v) : String(v);
	}
	return out;
}
