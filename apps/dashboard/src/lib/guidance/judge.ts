/**
 * Three states, judged only when judgeable (docs/roast-guidance.md, principle 4): waiting (no
 * value yet), within, or outside (below / above).
 */
import type { Expectation } from './expect.js';

type JudgeState = 'waiting' | 'within' | 'below' | 'above';

export interface Judgement {
	state: JudgeState;
	value: number | null;
	range: [number, number];
	source: Expectation['source'];
	exp: Expectation;
}

/** null when there is nothing to judge against. */
export function judge(
	value: number | null | undefined,
	exp: Expectation | null | undefined
): Judgement | null {
	if (!exp) return null;
	const v = value != null && Number.isFinite(value) ? value : null;
	const [lo, hi] = exp.range;
	const state: JudgeState = v == null ? 'waiting' : v < lo ? 'below' : v > hi ? 'above' : 'within';
	return { state, value: v, range: exp.range, source: exp.source, exp };
}

export function isOutside(j: Judgement | null | undefined): boolean {
	return j?.state === 'below' || j?.state === 'above';
}
