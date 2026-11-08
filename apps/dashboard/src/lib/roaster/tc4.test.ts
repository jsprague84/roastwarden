import { describe, expect, it } from 'vitest';
import {
	chanLine,
	defaultTc4Settings,
	replyShape,
	tc4Config,
	tc4Problems,
	tc4Summary
} from './tc4.js';

describe('TC4 settings', () => {
	it('starts a new connection on CHAN;1200, stock aArtisan replies and the fan on IO3', () => {
		const s = defaultTc4Settings();
		expect(chanLine(s)).toBe('CHAN;1200');
		expect(s.reply_format).toBe('active');
		expect(s.fan_output).toBe('io3');
		expect(tc4Problems(s)).toEqual([]);
	});

	it('expects only the active channels in an aArtisan reply', () => {
		const s = defaultTc4Settings();
		expect(replyShape(s)).toBe('ambient,ET,BT,heater,fan,[SV]');
		expect(replyShape({ ...s, t3_channel: 3 })).toBe('ambient,ET,BT,T3,heater,fan,[SV]');
		expect(replyShape({ ...s, et_channel: 0, bt_channel: 1 })).toBe('ambient,BT,heater,fan,[SV]');
		expect(replyShape({ ...s, reply_format: 'fixed' })).toBe(
			'ambient,T1 (ET),T2 (BT),T3,T4,heater,fan,[SV]'
		);
	});

	it('maps every logical channel in CHAN', () => {
		const s = {
			...defaultTc4Settings(),
			et_channel: 3,
			bt_channel: 1,
			t3_channel: 2,
			t4_channel: 4
		};
		expect(chanLine(s)).toBe('CHAN;3124');
		expect(tc4Config(s)).toMatchObject({
			et_channel: 3,
			bt_channel: 1,
			t3_channel: 2,
			t4_channel: 4
		});
	});

	it('says what is wrong', () => {
		const s = defaultTc4Settings();
		expect(tc4Problems({ ...s, et_channel: 0, bt_channel: 0 })).toContain(
			'ET or BT needs a channel.'
		);
		expect(tc4Problems({ ...s, t3_channel: 2 })).toContain(
			'Each physical input can feed one channel only.'
		);
		expect(tc4Problems({ ...s, t4_channel: 5 })).toContain('T4 channel must be 0 (off) to 4.');
		expect(tc4Problems({ ...s, port: ' ' })).toContain('A serial port is needed.');
	});

	it('reads an old config the way the server does', () => {
		expect(tc4Summary({ port: '/dev/ttyACM0', baud: 115200, et_channel: 1, bt_channel: 2 })).toBe(
			'/dev/ttyACM0 @ 115200 · CHAN;1200 · fixed replies · fan IO3'
		);
		expect(
			tc4Summary(tc4Config({ ...defaultTc4Settings(), fan_output: 'ot2', t3_channel: 3 }))
		).toBe('/dev/ttyUSB0 @ 115200 · CHAN;1230 · aArtisan replies · fan OT2');
	});
});
