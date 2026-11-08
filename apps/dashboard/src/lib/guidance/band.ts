/**
 * Geometry and hint text for RangeBand (docs/roast-guidance.md, "Dashboard"): a thin bar with
 * the expected range and a marker for the value.
 */
import type { Judgement } from './judge.js';
import type { Help } from '#lib/constants/help.js';
import { sourceById } from './references.js';

export interface BandGeometry {
	/** Range start and end, % of the bar. */
	lo: number;
	hi: number;
	/** Value marker, % of the bar; null while waiting. */
	at: number | null;
}

/** The range sits in the middle half of the bar; the scale widens to keep the value on it. */
export function bandGeometry(range: [number, number], value: number | null): BandGeometry {
	const [lo, hi] = range;
	const pad = Math.max((hi - lo) * 0.5, Math.abs(hi) * 0.02, 1e-6);
	let min = lo - pad;
	let max = hi + pad;
	if (value != null) {
		min = Math.min(min, value - pad * 0.2);
		max = Math.max(max, value + pad * 0.2);
	}
	const pct = (x: number) => ((x - min) / (max - min)) * 100;
	return { lo: pct(lo), hi: pct(hi), at: value == null ? null : pct(value) };
}

export interface BandHelp {
	help: Help;
	links: { title: string; url: string }[];
}

/**
 * "Light expects 14–16 %." with the basis and sources. `range` formats the range in display
 * units; `extra` adds lines (DTR, the level a loss suggests, the moisture note).
 */
export function bandHelp(
	j: Judgement,
	range: string,
	levelLabel: string,
	extra: string[] = []
): BandHelp {
	const e = j.exp;
	const who =
		e.source === 'profile'
			? 'The profile expects'
			: e.source === 'history'
				? `This profile’s last ${e.n} roasts here ran`
				: `${levelLabel} expects`;
	const state =
		j.state === 'waiting'
			? ''
			: j.state === 'within'
				? ' Within.'
				: j.state === 'below'
					? ' Below.'
					: ' Above.';
	const more = [e.basis];
	if (e.derived) more.push('A Roastwarden default from the cited points, not a quoted standard.');
	more.push(...extra);
	const links = e.sources
		.map((id) => sourceById(id))
		.filter((s) => s != null)
		.map((s) => ({ title: `${s.title}${s.type === 'c' ? ' (community)' : ''}`, url: s.url }));
	return { help: { line: `${who} ${range}.${state}`, more: more.join(' ') }, links };
}
