import { describe, expect, it } from 'vitest';
import type { ContractView } from '#lib/api/roasters.js';
import { contractLines } from './contract.js';

const base: ContractView = {
	device_id: 'esp32_roaster_01',
	firmware: '3.5.0',
	reads: { bt: true, et: true, heater: true, fan: true },
	caps: { pid: true, ff: true, lease: true, tpc: true, ror: false, reported: true },
	gains: {
		reported: { kp: 5.561, ki: 0.08471, kd: 0 },
		held: { kp: 5.561, ki: 0.08471, kd: 0 },
		matches: true,
		last_boot: {
			at: '2026-10-27T10:42:11Z',
			why: 'reboot',
			reported: { kp: 5.5, ki: 0.085, kd: 0 },
			held: { kp: 5.561, ki: 0.08471, kd: 0 },
			sent: { kp: 5.561, ki: 0.08471, kd: 0 }
		}
	}
};

describe('contractLines', () => {
	it('says the gains match and what the boot check sent', () => {
		const l = contractLines(base);
		expect(l.reads.ok).toBe(true);
		expect(l.caps).toBe(
			'onboard PID, feedforward, control lease, time-proportioned heater (as it reports them)'
		);
		expect(l.gains).toEqual({
			ok: true,
			text: 'Kp 5.561 · Ki 0.08471 · Kd 0: the ones the server holds'
		});
		expect(l.boot).toMatch(
			/^After its last reboot .* it had Kp 5.5 · Ki 0.085 · Kd 0; the server sent Kp 5.561/
		);
	});

	it('names missing readings and differing gains', () => {
		const l = contractLines({
			...base,
			reads: { bt: true, et: false, heater: true, fan: false },
			caps: { ...base.caps, reported: false },
			gains: {
				...base.gains,
				reported: { kp: 5.5, ki: 0.085, kd: 0 },
				matches: false,
				last_boot: null
			}
		});
		expect(l.reads).toEqual({ ok: false, text: 'its latest sample lacks ET, fan' });
		expect(l.caps).toMatch(/inferred/);
		expect(l.gains.ok).toBe(false);
		expect(l.gains.text).toBe(
			'reports Kp 5.5 · Ki 0.085 · Kd 0, the server holds Kp 5.561 · Ki 0.08471 · Kd 0'
		);
		expect(l.boot).toBeNull();
	});

	it('a roaster with no held gains is not called wrong', () => {
		const l = contractLines({
			...base,
			gains: { reported: base.gains.reported, held: null, matches: null }
		});
		expect(l.gains).toEqual({
			ok: null,
			text: 'reports Kp 5.561 · Ki 0.08471 · Kd 0; the server holds none for it'
		});
	});
});
