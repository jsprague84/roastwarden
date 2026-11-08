/**
 * What a roast is expected to show (docs/roast-guidance.md): the profile's own ranges first,
 * then its roast level's defaults (references.ts), then this roaster's own history of the
 * profile. Temperatures and times come from own history only.
 */
import {
	defaultsFor,
	isLevel,
	levelLabel,
	moistureCorrected,
	toAgtron,
	BASE_MOISTURE,
	type LevelKey,
	type MetricKey
} from './references.js';

/** One completed roast of the profile, as `GET /api/profiles/{id}/history` returns it.
 * Times are seconds from CHARGE, temperatures °C. */
export interface HistoryRoast {
	session_id: string;
	name: string;
	device_id: string;
	batch_number: number | null;
	roasted_at: string;
	fc_s: number | null;
	fc_bt: number | null;
	drop_s: number | null;
	drop_bt: number | null;
	dry_s: number | null;
	dry_bt: number | null;
	development_s: number | null;
	delta_t_fc_drop: number | null;
	total_s: number | null;
	weight_loss_pct: number | null;
	color_ground: number | null;
	color_scale: string | null;
	rmse: number | null;
}

type ExpectSource = 'profile' | 'default' | 'history';

export interface Expectation {
	range: [number, number];
	source: ExpectSource;
	/** What the range rests on, for the hint. */
	basis: string;
	/** Source ids (references.ts). */
	sources: string[];
	/** A Roastwarden default built from cited points. */
	derived?: boolean;
	/** Roasts behind a history band. */
	n?: number;
}

/** Measures judged against own history only. */
type HistoryKey = 'fc_s' | 'fc_bt' | 'drop_bt' | 'dry_bt' | 'delta_t_fc_drop' | 'total_s';

/** Own-history bands need this many roasts (the roast being judged excluded). */
export const MIN_HISTORY = 3;

/** Half-width floors for history bands, so a few near-identical roasts don't make a band
 * nothing fits: temperature °C, time s, loss points, colour points. */
const FLOOR = { temp: 2, time: 15, loss: 0.5, colour: 2 };

const HISTORY_FLOORS: Record<HistoryKey | 'weight_loss_pct' | 'color_ground', number> = {
	fc_s: FLOOR.time,
	total_s: FLOOR.time,
	fc_bt: FLOOR.temp,
	drop_bt: FLOOR.temp,
	dry_bt: FLOOR.temp,
	delta_t_fc_drop: FLOOR.temp,
	weight_loss_pct: FLOOR.loss,
	color_ground: FLOOR.colour
};

/**
 * mean ± max(2σ, floor) of the values; null below MIN_HISTORY values. Every measure here is
 * a time, a temperature, a loss or a colour, never below zero: a band that would start below
 * zero (one odd roast widens σ) starts at the smallest value seen instead (the server's
 * `expect::history_band` does the same).
 */
export function historyBand(values: number[], floor: number): [number, number] | null {
	const v = values.filter((x) => Number.isFinite(x));
	if (v.length < MIN_HISTORY) return null;
	const mean = v.reduce((a, b) => a + b, 0) / v.length;
	const sd = Math.sqrt(v.reduce((a, b) => a + (b - mean) ** 2, 0) / (v.length - 1));
	const half = Math.max(2 * sd, floor);
	const lo = mean - half;
	return [lo < 0 ? Math.max(0, Math.min(...v)) : lo, mean + half];
}

export interface Expectations
	extends Record<MetricKey, Expectation | null>, Record<HistoryKey, Expectation | null> {
	level: LevelKey | null;
	/** Roasts of this profile on this roaster, the judged one excluded. */
	historyCount: number;
	/** Green moisture the loss range was corrected for, %. */
	moisture: number | null;
}

export interface ProfileLike {
	roast_level?: string | null;
	expectations?: Partial<Record<MetricKey, [number, number]>> | null;
}

export interface SessionLike {
	id?: string;
	target_roast_level?: string | null;
	moisture_green?: number | null;
}

const OWN = 'this profile’s roasts on this roaster';

/**
 * Merged expected ranges for a roast: the profile's overrides, else the level defaults (loss
 * corrected for green moisture), else own history for loss and colour. `history` is the
 * profile's roasts on this roaster; the session itself is left out.
 */
export function expectationsFor(
	profile: ProfileLike | null | undefined,
	session: SessionLike | null | undefined,
	history: HistoryRoast[] = []
): Expectations {
	const level = isLevel(profile?.roast_level)
		? profile!.roast_level!
		: isLevel(session?.target_roast_level)
			? session!.target_roast_level!
			: null;
	const label = levelLabel(level);
	const moisture =
		session?.moisture_green != null && Number.isFinite(session.moisture_green)
			? session.moisture_green
			: null;
	const past = history.filter((h) => h.session_id !== session?.id);
	const defaults = defaultsFor(level as LevelKey | null);

	const fromHistory = (
		key: keyof typeof HISTORY_FLOORS,
		pick: (h: HistoryRoast) => number | null
	): Expectation | null => {
		const values = past.map(pick).filter((x): x is number => x != null);
		const range = historyBand(values, HISTORY_FLOORS[key]);
		return range
			? {
					range,
					source: 'history',
					basis: `The usual range of ${OWN} (${values.length}).`,
					sources: [],
					n: values.length
				}
			: null;
	};

	const out = {
		level: level as LevelKey | null,
		historyCount: past.length,
		moisture
	} as Expectations;
	for (const key of [
		'weight_loss_pct',
		'development_s',
		'dtr_pct',
		'color_ground'
	] as MetricKey[]) {
		const own = profile?.expectations?.[key];
		const def = defaults[key];
		if (own && own.length === 2) {
			out[key] = {
				range: [own[0], own[1]],
				source: 'profile',
				basis: 'Set on the profile.',
				sources: def?.sources ?? []
			};
		} else if (def) {
			let range = def.range;
			let basis = `${label}: ${def.basis}`;
			if (
				key === 'weight_loss_pct' &&
				moisture != null &&
				Math.abs(moisture - BASE_MOISTURE) >= 0.05
			) {
				range = moistureCorrected(def.range, moisture);
				basis += ` Shifted for ${fmt1(moisture)} % green moisture (about 1 point per point from ${BASE_MOISTURE} %).`;
			}
			out[key] = { range, source: 'default', basis, sources: def.sources, derived: def.derived };
		} else if (key === 'weight_loss_pct') {
			out[key] = fromHistory(key, (h) => h.weight_loss_pct);
		} else if (key === 'color_ground') {
			out[key] = fromHistory(key, (h) => toAgtron(h.color_ground, h.color_scale).agtron);
		} else {
			out[key] = null;
		}
	}
	out.fc_s = fromHistory('fc_s', (h) => h.fc_s);
	out.total_s = fromHistory('total_s', (h) => h.total_s);
	out.fc_bt = fromHistory('fc_bt', (h) => h.fc_bt);
	out.drop_bt = fromHistory('drop_bt', (h) => h.drop_bt);
	out.dry_bt = fromHistory('dry_bt', (h) => h.dry_bt);
	out.delta_t_fc_drop = fromHistory('delta_t_fc_drop', (h) => h.delta_t_fc_drop);
	return out;
}

/** "No other roasts of this profile on this roaster yet; …" while history is too thin. */
export function historyNote(n: number): string | null {
	if (n >= MIN_HISTORY) return null;
	const roasts = n === 0 ? 'No other roasts' : n === 1 ? '1 other roast' : `${n} other roasts`;
	return `${roasts} of this profile on this roaster${n === 0 ? ' yet' : ''}; temperatures and times are judged from ${MIN_HISTORY}.`;
}

function fmt1(x: number): string {
	return String(Math.round(x * 10) / 10);
}
