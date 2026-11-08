/**
 * Artisan alarm files (`.alrm`) to and from Roastwarden alarm rules (docs/alarms.md,
 * "Artisan alarm files").
 *
 * An `.alrm` is JSON with one list per column, a row per alarm (Artisan's `alarms.py`,
 * `exportalarmsJSON`; evaluation in `canvas.py`):
 *
 * - `alarmflags`: 1 on, 0 off.
 * - `alarmguards` / `alarmnegguards`: row index (0-based) that must / must not have fired; -1 none.
 * - `alarmtimes` ("From"): -1 START, 0 CHARGE, 1 DRY END, 2 FC START, 3 FC END, 4 SC START,
 *   5 SC END, 6 DROP, 7 COOL END, 8 TP, 9 ON (always), 10 If Alarm (the guard's firing).
 * - `alarmoffsets`: seconds after From. Artisan fires a row when the offset has passed
 *   *or* the temperature condition holds.
 * - `alarmsources`: -3 none (time only), -2 ΔET, -1 ΔBT, 0 ET, 1 BT, 2+ extra devices.
 * - `alarmconds`: 0 below, 1 above, 2 equal, 3 not equal.
 * - `alarmtemperatures`: threshold in the unit Artisan ran in (the file does not say which).
 * - `alarmactions`: -1 none, 0 pop up, 1 call program, 2 event button, 3–6 slider of event
 *   type 1–4 (Air, Drum, Damper, Burner by default), 7 START, 8 DRY END, 9 FC START, 10 FC END,
 *   11 SC START, 12 SC END, 13 DROP, 14 COOL END, 15 OFF, 16 CHARGE, 17/18 ramp-soak on/off,
 *   19 PID ON, 20 PID OFF, 21 SV, 22/23 playback on/off, 24/25 canvas colour.
 * - `alarmbeep`: 1 beep. `alarmstrings`: the action's argument (pop-up text, slider value, SV).
 *
 * Artisan has one action per row and no names; Roastwarden rules have names and action lists.
 * Export splits a rule with several actions into a chain of "If Alarm" rows, Artisan's own
 * idiom; import folds such a chain back into one rule. Anything that does not carry over is
 * reported row by row rather than guessed.
 */
import type { AlarmAction, AlarmRule } from './alarms.js';

export type TempUnit = 'C' | 'F';

export interface AlrmFile {
	alarmflags: number[];
	alarmguards: number[];
	alarmnegguards: number[];
	alarmtimes: number[];
	alarmoffsets: number[];
	alarmconds: number[];
	alarmsources: number[];
	alarmtemperatures: number[];
	alarmactions: number[];
	alarmbeep: number[];
	alarmstrings: string[];
}

/** What did not carry over. `row` is Artisan's row number (1-based) or the rule's name. */
export interface AlrmNote {
	at: string;
	/** skipped: nothing imported or exported for it; partial: kept, but something was dropped. */
	kind: 'skipped' | 'partial';
	text: string;
}

const FROM_TO_LANDMARK: Record<number, string> = {
	0: 'charge',
	1: 'drying_end',
	2: 'first_crack_start',
	3: 'first_crack_end',
	4: 'second_crack_start',
	5: 'second_crack_end',
	6: 'drop',
	7: 'cool_end',
	8: 'turning_point'
};
const LANDMARK_TO_FROM: Record<string, number> = Object.fromEntries(
	Object.entries(FROM_TO_LANDMARK).map(([k, v]) => [v, Number(k)])
);
const FROM_LABEL: Record<number, string> = {
	[-1]: 'START',
	0: 'CHARGE',
	1: 'DRY END',
	2: 'FC START',
	3: 'FC END',
	4: 'SC START',
	5: 'SC END',
	6: 'DROP',
	7: 'COOL END',
	8: 'TP',
	9: 'ON',
	10: 'If Alarm'
};

/** Landmark actions: Artisan action index ↔ Roastwarden landmark. */
const MARK_ACTIONS: Record<number, string> = {
	8: 'drying_end',
	9: 'first_crack_start',
	10: 'first_crack_end',
	11: 'second_crack_start',
	12: 'second_crack_end',
	13: 'drop',
	14: 'cool_end',
	16: 'charge'
};
const LANDMARK_TO_MARK: Record<string, number> = Object.fromEntries(
	Object.entries(MARK_ACTIONS).map(([k, v]) => [v, Number(k)])
);
const UNMAPPED_ACTIONS: Record<number, string> = {
	1: 'Call Program',
	2: 'Event Button',
	4: 'Slider of event type 2 (Drum by default)',
	5: 'Slider of event type 3 (Damper by default)',
	7: 'START',
	15: 'OFF',
	17: 'RampSoak ON',
	18: 'RampSoak OFF',
	22: 'Playback ON',
	24: 'Set Canvas Color',
	25: 'Reset Canvas Color'
};
const SLIDER_AIR = 3;
const SLIDER_BURNER = 6;
const POP_UP = 0;
const PID_ON = 19;
const PID_OFF = 20;
const SV = 21;
const PLAYBACK_OFF = 23;
const IF_ALARM = 10;
const START = -1;
const NO_SOURCE = -3;

const tempIn = (v: number, u: TempUnit) => (u === 'F' ? (v - 32) / 1.8 : v);
const tempOut = (c: number, u: TempUnit) => (u === 'F' ? c * 1.8 + 32 : c);
const rateIn = (v: number, u: TempUnit) => (u === 'F' ? v / 1.8 : v);
const rateOut = (c: number, u: TempUnit) => (u === 'F' ? c * 1.8 : c);
const round1 = (v: number) => Math.round(v * 10) / 10;

/** True when parsed JSON looks like an Artisan alarm file. */
export function isAlrm(obj: unknown): obj is Partial<AlrmFile> {
	return (
		!!obj && typeof obj === 'object' && Array.isArray((obj as Record<string, unknown>).alarmflags)
	);
}

function column<T>(f: Partial<AlrmFile>, key: keyof AlrmFile, n: number, fill: T): T[] {
	const v = (f as Record<string, unknown>)[key];
	const arr = Array.isArray(v) ? (v as T[]) : [];
	return Array.from({ length: n }, (_, i) => (i < arr.length ? arr[i] : fill));
}

function percent(s: string): number | null {
	const v = Number(String(s).trim());
	return Number.isFinite(v) && v >= 0 && v <= 100 ? v : null;
}

/** One row's action as a Roastwarden action, or why it has none. */
function actionFromRow(
	code: number,
	arg: string,
	unit: TempUnit
): { action?: AlarmAction; note?: string } {
	if (code === -1 || code === POP_UP) return {};
	if (code in MARK_ACTIONS) return { action: { type: 'mark', event: MARK_ACTIONS[code] } };
	if (code === SLIDER_AIR || code === SLIDER_BURNER) {
		const v = percent(arg);
		if (v == null) return { note: `slider value "${arg}" is not a percentage` };
		return { action: { type: code === SLIDER_AIR ? 'fan' : 'heater', value: v } };
	}
	if (code === PID_ON) return { action: { type: 'mode', mode: 'auto' } };
	if (code === PID_OFF) return { action: { type: 'mode', mode: 'manual' } };
	if (code === SV) {
		const v = Number(String(arg).trim());
		if (!Number.isFinite(v)) return { note: `SV value "${arg}" is not a number` };
		return { action: { type: 'setpoint', value: round1(tempIn(v, unit)) } };
	}
	if (code === PLAYBACK_OFF) return { action: { type: 'replay_stop' } };
	return { note: `action ${UNMAPPED_ACTIONS[code] ?? `#${code}`} has no Roastwarden equivalent` };
}

/**
 * Artisan rows to Roastwarden rules. `unit` is the unit Artisan ran in (the file does not
 * record it). Rows chained by "If Alarm" with no condition of their own and no delay fold
 * into the rule they hang off, as extra actions.
 */
export function fromAlrm(
	file: Partial<AlrmFile>,
	unit: TempUnit
): { rules: AlarmRule[]; notes: AlrmNote[] } {
	const n = Array.isArray(file.alarmflags) ? file.alarmflags.length : 0;
	const flags = column(file, 'alarmflags', n, 1);
	const guards = column(file, 'alarmguards', n, -1);
	const negGuards = column(file, 'alarmnegguards', n, -1);
	const times = column(file, 'alarmtimes', n, START);
	const offsets = column(file, 'alarmoffsets', n, 0);
	const conds = column(file, 'alarmconds', n, 1);
	const sources = column(file, 'alarmsources', n, 1);
	const temps = column(file, 'alarmtemperatures', n, 0);
	const actions = column(file, 'alarmactions', n, -1);
	const strings = column(file, 'alarmstrings', n, '');
	const notes: AlrmNote[] = [];
	const at = (i: number) => `row ${i + 1}`;

	// A follow-on row: fires right after its guard and only acts.
	const followOn = (i: number) =>
		times[i] === IF_ALARM &&
		guards[i] >= 0 &&
		guards[i] < n &&
		sources[i] === NO_SOURCE &&
		!(offsets[i] > 0) &&
		!(negGuards[i] >= 0);
	// The head of a chain: follow guards back to a row that is not a follow-on.
	const headOf = (i: number): number => {
		let j = i;
		const seen = new Set<number>();
		while (followOn(j) && !seen.has(j)) {
			seen.add(j);
			j = guards[j];
		}
		return j;
	};

	const ruleOf = new Map<number, AlarmRule>();
	const names = new Set<string>();
	const uniqueName = (base: string) => {
		let name = base.trim() || 'Alarm';
		for (let k = 2; names.has(name); k++) name = `${base} (${k})`;
		names.add(name);
		return name;
	};

	for (let i = 0; i < n; i++) {
		if (followOn(i)) continue;
		const src = sources[i];
		const cond = conds[i];
		const from = times[i];
		const offset = Math.max(0, Number(offsets[i]) || 0);
		const rule: AlarmRule = { name: '', condition_type: '', threshold: 0, enabled: flags[i] !== 0 };
		let what: string;

		if (src === NO_SOURCE) {
			if (from === IF_ALARM) {
				notes.push({
					at: at(i),
					kind: 'skipped',
					text: `fires ${offset} s after alarm ${guards[i] + 1}: Roastwarden times only from landmarks`
				});
				continue;
			}
			rule.condition_type = 'time_after_event';
			rule.threshold = offset;
			rule.reference_event = FROM_TO_LANDMARK[from] ?? 'charge';
			if (!(from in FROM_TO_LANDMARK)) {
				notes.push({
					at: at(i),
					kind: 'partial',
					text: `time counted from ${FROM_LABEL[from] ?? from}; Roastwarden counts from CHARGE`
				});
			}
			what = `${Math.floor(offset / 60)}:${String(Math.round(offset % 60)).padStart(2, '0')} after ${FROM_LABEL[from] ?? 'CHARGE'}`;
		} else {
			if (src === -2 || src > 1) {
				notes.push({
					at: at(i),
					kind: 'skipped',
					text:
						src === -2
							? 'watches ΔET: Roastwarden alarms watch the RoR of BT only'
							: 'watches an extra device'
				});
				continue;
			}
			if (cond !== 0 && cond !== 1) {
				notes.push({
					at: at(i),
					kind: 'skipped',
					text: `condition "${cond === 2 ? '=' : '≠'}": Roastwarden has above and below`
				});
				continue;
			}
			if (from === IF_ALARM) {
				notes.push({
					at: at(i),
					kind: 'skipped',
					text: `compares against the reading when alarm ${guards[i] + 1} fired: no Roastwarden equivalent`
				});
				continue;
			}
			const above = cond === 1;
			const t = Number(temps[i]);
			if (src === 1) {
				rule.condition_type = above ? 'temp_above' : 'temp_below';
				rule.threshold = round1(tempIn(t, unit));
				what = `BT ${above ? '>' : '<'} ${t}`;
			} else if (src === 0) {
				rule.condition_type = above ? 'et_above' : 'et_below';
				rule.threshold = round1(tempIn(t, unit));
				what = `ET ${above ? '>' : '<'} ${t}`;
			} else {
				rule.condition_type = above ? 'ror_above' : 'ror_below';
				rule.threshold = round1(rateIn(t, unit));
				what = `RoR ${above ? '>' : '<'} ${t}`;
			}
			if (from in FROM_TO_LANDMARK && from !== 0) rule.armed_after = FROM_TO_LANDMARK[from];
			else if (from === 0) rule.armed_after = 'charge';
			if (offset > 0) {
				notes.push({
					at: at(i),
					kind: 'partial',
					text: `Artisan also fires it ${offset} s after ${FROM_LABEL[from] ?? 'From'}; imported on the temperature only`
				});
			}
		}
		if (negGuards[i] >= 0) {
			notes.push({
				at: at(i),
				kind: 'partial',
				text: `"only if alarm ${negGuards[i] + 1} has not fired" dropped: Roastwarden has no such guard`
			});
		}
		if (guards[i] >= 0) rule.requires = `#${headOf(guards[i])}`; // resolved to a name below

		const code = actions[i];
		const r = actionFromRow(code, strings[i], unit);
		if (r.note) notes.push({ at: at(i), kind: 'partial', text: r.note });
		rule.actions = r.action ? [r.action] : [];
		const label =
			code === POP_UP && String(strings[i]).trim()
				? String(strings[i]).trim()
				: `Artisan ${i + 1}: ${what}`;
		rule.name = uniqueName(label);
		ruleOf.set(i, rule);
	}

	// Fold follow-on rows into their chain's head, in row order.
	for (let i = 0; i < n; i++) {
		if (!followOn(i)) continue;
		const head = ruleOf.get(headOf(i));
		if (!head) {
			notes.push({
				at: at(i),
				kind: 'skipped',
				text: `follows alarm ${guards[i] + 1}, which did not import`
			});
			continue;
		}
		const r = actionFromRow(actions[i], strings[i], unit);
		if (r.note) notes.push({ at: at(i), kind: 'partial', text: r.note });
		if (r.action) head.actions = [...(head.actions ?? []), r.action];
	}

	// Guards point at row numbers; turn them into names, or drop rules whose guard is gone.
	const rules: AlarmRule[] = [];
	for (const [i, rule] of ruleOf) {
		if (rule.requires?.startsWith('#')) {
			const g = Number(rule.requires.slice(1));
			const target = ruleOf.get(g);
			if (!target) {
				notes.push({
					at: at(i),
					kind: 'skipped',
					text: `needs alarm ${g + 1}, which did not import`
				});
				continue;
			}
			rule.requires = target.name;
		}
		if (!rule.actions?.length) delete rule.actions;
		rules.push(rule);
	}
	notes.sort((a, b) => Number(a.at.slice(4)) - Number(b.at.slice(4)));
	return { rules, notes };
}

function rowForAction(
	a: AlarmAction,
	unit: TempUnit
): { code: number; arg: string } | { note: string } {
	switch (a.type) {
		case 'setpoint':
			return { code: SV, arg: String(round1(tempOut(a.value, unit))) };
		case 'fan':
			return { code: SLIDER_AIR, arg: String(a.value) };
		case 'heater':
			return { code: SLIDER_BURNER, arg: String(a.value) };
		case 'heater_off':
			return { code: SLIDER_BURNER, arg: '0' };
		case 'mode':
			return { code: a.mode === 'auto' ? PID_ON : PID_OFF, arg: '' };
		case 'mark':
			return a.event in LANDMARK_TO_MARK
				? { code: LANDMARK_TO_MARK[a.event], arg: '' }
				: { note: `marking ${a.event} has no Artisan action` };
		case 'replay_stop':
			return { code: PLAYBACK_OFF, arg: '' };
		case 'replay_pause':
			return { note: 'pausing the replay has no Artisan action' };
		case 'emergency_stop':
			return { note: 'emergency stop has no Artisan action (OFF only stops recording)' };
	}
}

/**
 * Roastwarden rules to an Artisan alarm file, temperatures in `unit`. A rule with several
 * actions becomes a chain: its condition row carries the first action, each further action
 * an "If Alarm" row guarded by the one before.
 */
export function toAlrm(rules: AlarmRule[], unit: TempUnit): { file: AlrmFile; notes: AlrmNote[] } {
	const file: AlrmFile = {
		alarmflags: [],
		alarmguards: [],
		alarmnegguards: [],
		alarmtimes: [],
		alarmoffsets: [],
		alarmconds: [],
		alarmsources: [],
		alarmtemperatures: [],
		alarmactions: [],
		alarmbeep: [],
		alarmstrings: []
	};
	const notes: AlrmNote[] = [];
	const push = (r: {
		flag: number;
		guard: number;
		time: number;
		offset: number;
		cond: number;
		source: number;
		temp: number;
		action: number;
		arg: string;
	}) => {
		file.alarmflags.push(r.flag);
		file.alarmguards.push(r.guard);
		file.alarmnegguards.push(-1);
		file.alarmtimes.push(r.time);
		file.alarmoffsets.push(r.offset);
		file.alarmconds.push(r.cond);
		file.alarmsources.push(r.source);
		file.alarmtemperatures.push(r.temp);
		file.alarmactions.push(r.action);
		file.alarmbeep.push(1);
		file.alarmstrings.push(r.arg);
		return file.alarmflags.length - 1;
	};

	// Rules that can be written, so `requires` knows which rows exist.
	const exportable = (r: AlarmRule) =>
		![
			'delta_above',
			'delta_below',
			'no_telemetry',
			'channel_above',
			'channel_below',
			'crash_ahead',
			'flick_ahead',
			'stall_ahead'
		].includes(r.condition_type);
	const pending = rules.filter((r) => {
		if (exportable(r)) return true;
		notes.push({
			at: r.name,
			kind: 'skipped',
			text:
				r.condition_type === 'no_telemetry'
					? 'lost telemetry is not an Artisan alarm condition'
					: r.condition_type.endsWith('_ahead')
						? "a warning from the roaster's own model: Artisan has no such alarm"
						: r.condition_type.startsWith('channel_')
							? "a declared channel: Artisan's extra devices are set up in Artisan"
							: 'BT against the profile is not an Artisan alarm condition'
		});
		return false;
	});
	const rowOfName = new Map<string, number>();
	// Rules that require another go after it, so its row number is known.
	const ordered: AlarmRule[] = [];
	const placed = new Set<string>();
	const byName = new Map(pending.map((r) => [r.name, r]));
	const place = (r: AlarmRule, depth = 0) => {
		if (placed.has(r.name) || depth > pending.length) return;
		if (r.requires && byName.has(r.requires)) place(byName.get(r.requires)!, depth + 1);
		placed.add(r.name);
		ordered.push(r);
	};
	pending.forEach((r) => place(r));

	for (const r of ordered) {
		let guard = -1;
		if (r.requires) {
			if (!rowOfName.has(r.requires)) {
				notes.push({
					at: r.name,
					kind: 'skipped',
					text: `needs "${r.requires}", which is not in the file`
				});
				continue;
			}
			guard = rowOfName.get(r.requires)!;
		}
		const armed =
			r.condition_type === 'time_after_event'
				? r.armed_after
				: (r.armed_after ?? r.reference_event);
		let time = START;
		let source: number;
		let cond = 1;
		let temp = 0;
		let offset = 0;
		const c = r.condition_type;
		if (c === 'time_after_event') {
			const ev = r.reference_event || 'charge';
			if (!(ev in LANDMARK_TO_FROM)) {
				notes.push({
					at: r.name,
					kind: 'skipped',
					text: `counts from ${ev}, which Artisan cannot time from`
				});
				continue;
			}
			time = LANDMARK_TO_FROM[ev];
			source = NO_SOURCE;
			offset = Math.max(0, Math.round(r.threshold));
			if (armed && armed !== ev)
				notes.push({
					at: r.name,
					kind: 'partial',
					text: `also armed after ${armed}: Artisan has one From per row`
				});
		} else {
			source = c.startsWith('temp_') ? 1 : c.startsWith('et_') ? 0 : -1;
			cond = c.endsWith('_above') ? 1 : 0;
			temp = round1(source === -1 ? rateOut(r.threshold, unit) : tempOut(r.threshold, unit));
			if (armed) {
				if (armed in LANDMARK_TO_FROM) time = LANDMARK_TO_FROM[armed];
				else
					notes.push({
						at: r.name,
						kind: 'partial',
						text: `armed after ${armed}: Artisan has no such From, exported from START`
					});
			}
		}
		if (r.repeat)
			notes.push({
				at: r.name,
				kind: 'partial',
				text: 'repeats in Roastwarden; Artisan fires an alarm once per roast'
			});

		const acts: { code: number; arg: string }[] = [];
		for (const a of r.actions ?? []) {
			const m = rowForAction(a, unit);
			if ('note' in m) notes.push({ at: r.name, kind: 'partial', text: m.note });
			else acts.push(m);
		}
		if (!acts.length) acts.push({ code: POP_UP, arg: r.name });
		const flag = r.enabled === false ? 0 : 1;
		let prev = push({
			flag,
			guard,
			time,
			offset,
			cond,
			source,
			temp,
			action: acts[0].code,
			arg: acts[0].arg
		});
		rowOfName.set(r.name, prev);
		for (const a of acts.slice(1)) {
			prev = push({
				flag,
				guard: prev,
				time: IF_ALARM,
				offset: 0,
				cond: 1,
				source: NO_SOURCE,
				temp: 0,
				action: a.code,
				arg: a.arg
			});
		}
	}
	return { file, notes };
}
