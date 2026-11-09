/** The roast sequence (docs/roast-sequence.md): what CHARGE and DROP do to the roaster. */

import { request, roasterPath } from '#lib/api/http.js';

export interface SequenceConfig {
	/** At CHARGE: heater power on, fan to charge_fan_pct. */
	charge_enabled: boolean;
	charge_fan_pct: number;
	/** At CHARGE: start Profile drive (live) when the roast has a profile. */
	charge_drive: boolean;
	/** Heater % the drive's PID starts from (manual first, then auto); 0: none. */
	charge_heater_pct: number;
	/** At DROP: stop the drive, heater power off, fan to cool_fan_pct. */
	drop_enabled: boolean;
	cool_fan_pct: number;
	/** Fan off and COOL END once BT and ET are below this, °C; 0 leaves the fan running. */
	cool_end_bt_c: number;
}

export interface SequenceView {
	device_id: string;
	config: SequenceConfig;
	/** The roaster is cooling after a DROP or a cancel. */
	cooling: boolean;
	/** The roast being cooled: its samples are stored until COOL END, Complete or not. */
	cooling_roast_id: string | null;
}

export const roastSequence = {
	get: (deviceId: string) => request<SequenceView>(`${roasterPath(deviceId)}/sequence`),
	put: (deviceId: string, config: SequenceConfig) =>
		request<SequenceView>(
			`${roasterPath(deviceId)}/sequence`,
			{ method: 'PUT', body: JSON.stringify(config) },
			true
		)
};
