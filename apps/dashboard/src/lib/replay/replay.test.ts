import { describe, it, expect } from 'vitest';
import {
	autoDropFact,
	autoDropLine,
	cutBanner,
	holdNote,
	driveTabLabel,
	profileTimeSec,
	fmtChannelValue,
	fmtEta,
	fmtTrigger,
	fmtLogEntry,
	profileHasHeater,
	profileHasFan,
	sortChannels
} from './replay.js';
import type { ProfileWithPoints } from '#lib/api/profiles.js';
import type { ReplayEventView } from '#lib/api/replay.js';

const temp = (c: number) => `${c.toFixed(0)}°`;
const delta = (c: number) => `${c.toFixed(0)}°`;

function ev(o: Partial<ReplayEventView>): ReplayEventView {
	return {
		id: 'e',
		action: 'fan',
		value: 60,
		label: null,
		trigger_kind: 'time',
		at_seconds: 150,
		at_bt: null,
		expected_t: 150,
		trigger: 'time',
		state: 'pending',
		delay_seconds: 0,
		eta_seconds: 42,
		to_go_c: null,
		...o
	};
}

describe('replay helpers', () => {
	it('formats channel values by unit', () => {
		expect(fmtChannelValue('setpoint', 185.04, temp)).toBe('185°');
		expect(fmtChannelValue('fan', 69.6, temp)).toBe('70%');
		expect(fmtChannelValue('heater', null, temp)).toBe('--');
	});
	it('formats ETAs, approximate for BT triggers', () => {
		expect(fmtEta(ev({}))).toBe('0:42');
		expect(fmtEta(ev({ trigger: 'bt', eta_seconds: 70 }))).toBe('~1:10');
		expect(fmtEta(ev({ trigger: 'bt', state: 'waiting', eta_seconds: 5 }))).toBe('0:05');
		expect(fmtEta(ev({ eta_seconds: null }))).toBe('?');
	});
	it('describes triggers with delays and degrees to go', () => {
		expect(fmtTrigger(ev({}), temp, delta)).toBe('at 2:30');
		expect(
			fmtTrigger(ev({ trigger: 'bt', at_bt: 160, to_go_c: 12.2, delay_seconds: 15 }), temp, delta)
		).toBe('BT 160°, 12° to go +15 s');
	});
	it('reads log entries', () => {
		const base = { elapsed: 0, live_t: 95, profile_t: 95 };
		expect(fmtLogEntry({ ...base, kind: 'fired', action: 'fan', value: 60 }, temp)).toBe(
			'1:35 Fan 60%'
		);
		expect(
			fmtLogEntry({ ...base, kind: 'paused', channel: 'fan', detail: 'manual fan command' }, temp)
		).toBe('1:35 Fan paused (manual fan command)');
		expect(
			fmtLogEntry({ ...base, kind: 'engage', detail: 'engaged: replay drives the roaster' }, temp)
		).toBe('1:35 engaged: replay drives the roaster');
	});
	it('knows what a profile can drive', () => {
		const p = {
			points: [{ heater_pct: null, fan_speed: 70 }],
			events: [{ action: 'heater' }]
		} as unknown as ProfileWithPoints;
		expect(profileHasHeater(p)).toBe(true);
		expect(profileHasFan(p)).toBe(true);
		expect(
			profileHasHeater({
				points: [{ heater_pct: null }],
				events: []
			} as unknown as ProfileWithPoints)
		).toBe(false);
		expect(profileHasFan(null)).toBe(false);
	});
	it('puts driven channels first', () => {
		const chs = sortChannels([
			{ channel: 'setpoint', driven: false, paused: false, target: null, last_sent: null },
			{ channel: 'fan', driven: true, paused: false, target: 1, last_sent: 1 },
			{ channel: 'heater', driven: true, paused: false, target: 1, last_sent: 1 }
		]);
		expect(chs.map((c) => c.channel)).toEqual(['heater', 'fan', 'setpoint']);
	});
	it('measures profile time from CHARGE plus the align offset', () => {
		expect(profileTimeSec(100_000, 0, [])).toBe(100);
		expect(profileTimeSec(100_000, 0, [{ type: 'charge', elapsed_seconds: 90 }])).toBe(10);
		expect(profileTimeSec(100_000, 0, [{ type: 'charge', elapsed_seconds: 40 }], -20)).toBe(80);
	});
});

describe('driveTabLabel', () => {
	const st = (o: { ghost?: boolean; paused?: boolean; session?: string; active?: boolean } = {}) =>
		({
			active: o.active ?? true,
			session_id: o.session ?? 's1',
			profile_t: 252.4,
			config: { ghost: o.ghost ?? false },
			channels: [{ channel: 'fan', driven: true, paused: o.paused ?? false }]
		}) as unknown as Parameters<typeof driveTabLabel>[0];
	it('names the tab when nothing drives this session', () => {
		expect(driveTabLabel(null, 's1')).toEqual({ label: 'Profile drive', status: null, tone: null });
		expect(driveTabLabel(st({ session: 'other' }), 's1').status).toBeNull();
		expect(driveTabLabel(st({ active: false }), 's1').status).toBeNull();
	});
	it('shows live, ghost or paused with the profile time', () => {
		expect(driveTabLabel(st(), 's1')).toEqual({
			label: 'Drive',
			status: 'Live 4:12',
			tone: 'live'
		});
		expect(driveTabLabel(st({ ghost: true }), 's1').status).toBe('Ghost 4:12');
		expect(driveTabLabel(st({ paused: true }), 's1')).toEqual({
			label: 'Drive',
			status: 'Paused 4:12',
			tone: 'paused'
		});
	});
});

describe('cutBanner', () => {
	const st = (
		o: Partial<{
			active: boolean;
			stopped_by_device: boolean;
			stopped_reason: string;
			session_id: string;
		}> = {}
	) =>
		({
			active: false,
			session_id: 's1',
			stopped_by_device: true,
			stopped_reason: 'the device disabled its heater (safe state or operator)',
			live_t: 320.4,
			...o
		}) as Parameters<typeof cutBanner>[0];
	it('shows when the roaster cut the heater on this roast, with the firmware reason when known', () => {
		expect(cutBanner(st(), 's1', false)).toEqual({
			at: '5:20',
			why: 'safe state, emergency stop or an interlock',
			restart: false,
			profileAt: '0:00'
		});
		expect(
			cutBanner(
				st({
					stopped_reason:
						'the roaster cut its heater: control lease expired (the server went quiet)'
				}),
				's1',
				false
			)?.why
		).toBe('control lease expired (the server went quiet)');
	});
	it('shows a drive a server restart cut, with the profile time a resume picks up at', () => {
		const b = cutBanner(
			{
				...st({ stopped_by_device: false, stopped_reason: 'the server restarted' }),
				stopped_by_restart: true,
				profile_t: 290
			} as never,
			's1',
			false
		);
		expect(b).toEqual({
			at: '5:20',
			why: 'the server restarted',
			restart: true,
			profileAt: '4:50'
		});
		expect(
			cutBanner(
				{ ...st({ stopped_by_device: false }), stopped_by_restart: true } as never,
				's1',
				true
			)
		).toBeNull();
	});
	it('notes the lost time on a resumed run', () => {
		expect(holdNote({ config: { hold: { seconds: 40, since_elapsed: 400 } } } as never)).toBe(
			'Resumed after a restart: the profile runs 0:40 later for the time lost'
		);
		expect(holdNote({ config: {} } as never)).toBeNull();
	});
	it('stays away otherwise', () => {
		expect(cutBanner(st({ active: true }), 's1', false)).toBeNull();
		expect(cutBanner(st({ stopped_by_device: false }), 's1', false)).toBeNull();
		expect(cutBanner(st(), 'other', false)).toBeNull();
		expect(cutBanner(st(), 's1', true)).toBeNull();
		expect(cutBanner(null, 's1', false)).toBeNull();
	});
});

describe('autoDropLine', () => {
	const ad = (o: Record<string, unknown> = {}) => ({
		by: 'time',
		at: 605,
		countdown_seconds: 10,
		cancelled: false,
		state: 'armed',
		eta_seconds: 95,
		drop_at_elapsed: null,
		waiting_for: null,
		ghost: false,
		describe: "at the profile's drop, 10:05",
		...o
	});
	const st = (a: Record<string, unknown> | null, o: Record<string, unknown> = {}) =>
		({ active: true, session_id: 's1', config: { ghost: false }, auto_drop: a, ...o }) as never;

	it('says when it drops, then counts down', () => {
		expect(autoDropLine(st(ad()), 's1', false)).toMatchObject({
			state: 'armed',
			text: "Auto DROP at the profile's drop, 10:05 · 1:35 to go"
		});
		expect(
			autoDropLine(
				st(
					ad({
						by: 'development',
						describe: '2:15 after first crack',
						waiting_for: 'first crack',
						eta_seconds: null
					})
				),
				's1',
				false
			)?.text
		).toBe('Auto DROP 2:15 after first crack · after first crack');
		expect(
			autoDropLine(st(ad({ state: 'counting', eta_seconds: 6.2 })), 's1', false)
		).toMatchObject({ state: 'counting', text: 'Auto DROP in 7 s' });
		expect(autoDropLine(st(ad({ state: 'cancelled', cancelled: true })), 's1', false)?.text).toBe(
			'Auto DROP cancelled for this roast'
		);
	});

	it('shows nothing without an auto DROP, for another roast, a ghost, a stopped drive or after DROP', () => {
		expect(autoDropLine(st(null), 's1', false)).toBeNull();
		expect(autoDropLine(st(ad()), 's2', false)).toBeNull();
		expect(autoDropLine(st(ad({ ghost: true })), 's1', false)).toBeNull();
		expect(autoDropLine(st(ad(), { active: false }), 's1', false)).toBeNull();
		expect(autoDropLine(st(ad()), 's1', true)).toBeNull();
	});
});

describe('autoDropFact', () => {
	const p = { target_total_time: 605, target_first_crack: 470, target_end_temp: 210 };
	const t = (c: number) => `${c} °C`;
	it('names what the profile drops by', () => {
		expect(autoDropFact({ ...p, auto_drop: 'time' }, t)).toBe('auto DROP at 10:05');
		expect(autoDropFact({ ...p, auto_drop: 'bt' }, t)).toBe('auto DROP at 210 °C');
		expect(autoDropFact({ ...p, auto_drop: 'development' }, t)).toBe(
			'auto DROP 2:15 after first crack'
		);
		expect(autoDropFact({ ...p, auto_drop: null }, t)).toBeNull();
	});
});
