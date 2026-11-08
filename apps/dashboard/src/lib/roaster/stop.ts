/**
 * What the header's safety button is (docs/ui-structure.md, "App shell"):
 *
 * - **STOP** while there is something to stop: a roast open, the heater on, the drum turning,
 *   the server cooling after DROP, or the roaster (BT or ET) still above its cool-down
 *   temperature, the roast sequence's `cool_end_bt_c`.
 * - **Fan off** once the heater is off, nothing is open and the roaster has cooled below that,
 *   but the fan still runs (after an emergency stop it runs at 100 %): one tap instead of the
 *   Roast page's slider. Never while hot: the fan is what cools it.
 * - Nothing on a cold idle roaster (STOP would only run the fan at full), a roaster silent for
 *   15 s (a command would not reach it), or one whose control is off.
 */
import type { Telemetry } from '#lib/api/telemetry.js';

/** The cool-down temperature when the roaster's sequence has none (0 or not loaded), °C. */
export const STOP_HOT_C = 50;
/** No reading for this long: the roaster is not there. */
export const STOP_STALE_MS = 15_000;

export interface StopInputs {
	/** The selected roaster's newest reading */
	reading: Telemetry | null;
	/** When it arrived (ms, this browser's clock) */
	heardAt: number | null;
	now: number;
	/** A roast is open on it (active or paused) */
	roastOpen: boolean;
	/** Its setup lets Roastwarden send a stop and the fan (control manual or full) */
	controllable: boolean;
	/** The roast sequence's cool-down temperature, °C (`cool_end_bt_c`; 0 or null: none) */
	coolEndC?: number | null;
	/** The server is cooling it after a DROP or a cancel (it turns the fan off itself) */
	cooling?: boolean;
}

export type StopButton = { kind: 'stop'; why: string } | { kind: 'fan_off'; why: string } | null;

/** The cool-down temperature in force, °C. */
export function coolEnd(c: number | null | undefined): number {
	return c && c > 0 ? c : STOP_HOT_C;
}

/** Which button the header shows, and why. */
export function stopButton(i: StopInputs): StopButton {
	if (!i.controllable) return null;
	if (i.roastOpen) return { kind: 'stop', why: 'a roast is open' };
	const r = i.reading;
	if (!r || i.heardAt == null || i.now - i.heardAt > STOP_STALE_MS) return null;
	if (r.heaterEnable === 1 || r.heater > 0 || (r.heaterApplied ?? 0) > 0)
		return { kind: 'stop', why: 'the heater is on' };
	if ((r.drum ?? 0) > 0) return { kind: 'stop', why: 'the drum is turning' };
	const cool = coolEnd(i.coolEndC);
	if (i.cooling)
		return { kind: 'stop', why: `cooling after DROP: the fan goes off below ${cool} °C` };
	if (r.beanTemp >= cool || r.envTemp >= cool) {
		return {
			kind: 'stop',
			why: r.fan > 0 ? `cooling: Fan off once it is below ${cool} °C` : 'the roaster is hot'
		};
	}
	if (r.fan > 0) return { kind: 'fan_off', why: `the heater is off and it is below ${cool} °C` };
	return null;
}
