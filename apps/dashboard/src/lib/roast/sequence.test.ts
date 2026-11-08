import { describe, it, expect } from 'vitest';
import { chargeLine, dropLine } from './sequence.js';
import type { SequenceConfig } from '#lib/api/sequence.js';

const cfg: SequenceConfig = {
	charge_enabled: true,
	charge_fan_pct: 100,
	charge_drive: true,
	charge_heater_pct: 60,
	drop_enabled: true,
	cool_fan_pct: 100,
	cool_end_bt_c: 45
};
const temp = (c: number) => `${c} °C`;

describe('roast sequence lines', () => {
	it('says what Charge does, with the drive only when there is a profile', () => {
		expect(chargeLine(cfg, true)).toBe(
			'Charge also turns heater power on, sets the fan to 100 % and starts Profile drive from 60 % heater.'
		);
		expect(chargeLine(cfg, false)).toBe(
			'Charge also turns heater power on, sets the fan to 100 %.'
		);
		expect(chargeLine({ ...cfg, charge_enabled: false }, true)).toBeNull();
		expect(chargeLine(null, true)).toBeNull();
	});
	it('says what Drop does and when cooling ends', () => {
		expect(dropLine(cfg, temp)).toBe(
			'Drop stops the drive, cuts heater power (the roaster goes to Manual, 0 %) and cools at 100 % fan until BT and ET are below 45 °C, then the fan stops and Cool end is marked.'
		);
		expect(dropLine({ ...cfg, cool_end_bt_c: 0 }, temp)).toBe(
			'Drop stops the drive, cuts heater power (the roaster goes to Manual, 0 %) and cools at 100 % fan.'
		);
		expect(chargeLine({ ...cfg, charge_heater_pct: 0 }, true)).toBe(
			'Charge also turns heater power on, sets the fan to 100 % and starts Profile drive.'
		);
		expect(dropLine({ ...cfg, drop_enabled: false }, temp)).toBeNull();
	});
});
