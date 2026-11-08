import { describe, it, expect } from 'vitest';
import { assistSummary, isTrimming, statusText } from './fanAssist.js';
import type { FanAssistView } from '#lib/api/fanAssist.js';

const config = {
	enabled: true,
	min_fan_pct: 70,
	arm_bt_c: 150,
	high_heater_pct: 92,
	low_heater_pct: 80,
	max_down_pct_per_min: 15,
	max_up_pct_per_min: 6,
	lag_gain_pct_per_min_per_c: 2
};

function view(over: Partial<FanAssistView['state']> = {}, enabled = true): FanAssistView {
	return {
		device_id: 'r1',
		config: { ...config, enabled },
		state: { source: 'manual', active: true, reason: 'assisting', view: null, ...over }
	};
}

describe('fan assist summary', () => {
	it('is silent when off', () => {
		expect(assistSummary(null)).toBeNull();
		expect(assistSummary(view({}, false))).toBeNull();
	});

	it('explains why it is not acting', () => {
		expect(assistSummary(view({ active: false, reason: 'the heater is off' }))).toBe(
			'Fan assist on · the heater is off'
		);
	});

	it('shows the trim and the heater', () => {
		const v = view({
			view: {
				status: 'lowering',
				ceiling: 100,
				floor: 70,
				trim: 12.4,
				fan: 87.6,
				heater: 97.2,
				armed: true
			}
		});
		expect(assistSummary(v)).toBe(
			'Fan assist −12 % → 88 % · heater pinned: lowering the fan · heater 97 %'
		);
		expect(isTrimming(v)).toBe(true);
	});

	it('says the fan is at the setting with no trim', () => {
		const v = view({
			view: {
				status: 'at_ceiling',
				ceiling: 100,
				floor: 70,
				trim: 0,
				fan: 100,
				heater: 70,
				armed: true
			}
		});
		expect(assistSummary(v)).toBe('Fan assist on · fan at your setting · heater 70 %');
		expect(isTrimming(v)).toBe(false);
		expect(statusText('at_floor')).toContain('floor');
	});
});
