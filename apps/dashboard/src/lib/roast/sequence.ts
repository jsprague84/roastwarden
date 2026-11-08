/**
 * Words for the roast sequence (docs/roast-sequence.md): what CHARGE and DROP do to the
 * roaster, shown before you press them. Temperatures arrive in °C; callers pass a formatter.
 */
import type { SequenceConfig } from '#lib/api/sequence.js';

export type TempFmt = (c: number) => string;

/** What CHARGE will do, or null when the step is off. */
export function chargeLine(cfg: SequenceConfig | null, hasProfile: boolean): string | null {
	if (!cfg?.charge_enabled) return null;
	const drive = cfg.charge_drive && hasProfile;
	const from = drive && cfg.charge_heater_pct > 0 ? ` from ${cfg.charge_heater_pct} % heater` : '';
	return `Charge also turns heater power on, sets the fan to ${cfg.charge_fan_pct} %${drive ? ` and starts Profile drive${from}` : ''}.`;
}

/** What DROP will do, or null when the step is off. */
export function dropLine(cfg: SequenceConfig | null, temp: TempFmt): string | null {
	if (!cfg?.drop_enabled) return null;
	const until =
		cfg.cool_end_bt_c > 0
			? ` until BT and ET are below ${temp(cfg.cool_end_bt_c)}, then the fan stops and Cool end is marked`
			: '';
	return `Drop stops the drive, cuts heater power (the roaster goes to Manual, 0 %) and cools at ${cfg.cool_fan_pct} % fan${until}.`;
}
