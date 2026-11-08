import { describe, expect, it } from 'vitest';
import { busyLines } from './busy.js';

describe('busyLines', () => {
	it('is empty when idle or unknown', () => {
		expect(busyLines(null)).toEqual([]);
		expect(busyLines({ busy: false, roasters: [] })).toEqual([]);
	});
	it('names each roaster and what it is doing, and marks the virtual one', () => {
		expect(
			busyLines({
				busy: true,
				roasters: [
					{
						device_id: 'esp32_roaster_01',
						virtual: false,
						reasons: ['drive', 'heating'],
						bt_c: 180,
						session_id: 's1'
					},
					{
						device_id: 'virtual_roaster',
						virtual: true,
						reasons: ['cooling'],
						bt_c: 90,
						session_id: null
					}
				]
			})
		).toEqual([
			'esp32_roaster_01: profile drive, heater on',
			'virtual_roaster (virtual, does not hold deploys): cooling after DROP'
		]);
	});
});
