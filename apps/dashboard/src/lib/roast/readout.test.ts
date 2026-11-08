import { describe, it, expect } from 'vitest';
import { profileTrack, roastNumbers, deltaLevel, primaryAction } from './readout.js';
import type { ProfileWithPoints } from '#lib/api/profiles.js';

const profile = {
	id: 'p',
	name: 'Test',
	points: [
		{
			id: 'a',
			time_seconds: 0,
			target_temp: 30,
			fan_speed: null,
			notes: null,
			target_env_temp: null
		},
		{
			id: 'b',
			time_seconds: 600,
			target_temp: 210,
			fan_speed: null,
			notes: null,
			target_env_temp: null
		}
	],
	target_first_crack: 480,
	target_total_time: 600
} as unknown as ProfileWithPoints;

const start = '2026-10-22T11:00:00Z';
const t0 = new Date(start).getTime();
const ev = (event_type: string, elapsed_seconds: number) => ({ event_type, elapsed_seconds });

describe('roastNumbers', () => {
	const track = profileTrack(profile)!;

	it('reads the profile at the time since CHARGE', () => {
		// Charged 30 s in; now 330 s in → 300 s since charge → profile 120 °C.
		const n = roastNumbers({
			nowMs: t0 + 330_000,
			session: { start_time: start },
			events: [ev('charge', 30)],
			track,
			bt: 125
		});
		expect(n.sinceCharge).toBe(300);
		expect(n.profBt).toBeCloseTo(120, 6);
		expect(n.delta).toBeCloseTo(5, 6);
		expect(n.profRor).toBeCloseTo(18, 6);
		expect(n.devSec).toBeNull();
	});

	it('runs from the session start before CHARGE, and has no profile without a session', () => {
		const n = roastNumbers({
			nowMs: t0 + 60_000,
			session: { start_time: start },
			events: [],
			track,
			bt: 40
		});
		expect(n.sinceCharge).toBeNull();
		expect(n.profBt).toBeCloseTo(48, 6);
		expect(roastNumbers({ nowMs: t0, session: null, events: [], track, bt: 40 }).delta).toBeNull();
	});

	it('gives development time and DTR after first crack, frozen at DROP', () => {
		const events = [ev('charge', 0), ev('first_crack_start', 480)];
		const n = roastNumbers({
			nowMs: t0 + 540_000,
			session: { start_time: start },
			events,
			track,
			bt: 205
		});
		expect(n.devSec).toBe(60);
		expect(n.dtr).toBeCloseTo((60 / 540) * 100, 6);
		const d = roastNumbers({
			nowMs: t0 + 900_000,
			session: { start_time: start },
			events: [...events, ev('drop', 600)],
			track,
			bt: 150
		});
		expect(d.devSec).toBe(120);
		expect(d.dtr).toBeCloseTo(20, 6);
	});

	it('takes the profile development target', () => {
		expect(track.targetDtr).toBeCloseTo(20, 6);
	});
});

describe('deltaLevel', () => {
	it('grades the distance to the profile', () => {
		expect(deltaLevel(null)).toBeNull();
		expect(deltaLevel(-1.5)).toBe('ok');
		expect(deltaLevel(4)).toBe('warn');
		expect(deltaLevel(-8)).toBe('bad');
	});
});

describe('primaryAction', () => {
	it('leads each stage with one button', () => {
		expect(primaryAction(null, [])).toEqual({ kind: 'start' });
		expect(primaryAction({ status: 'active' }, [])).toEqual({ kind: 'mark', landmark: 'charge' });
		expect(primaryAction({ status: 'paused' }, [])).toEqual({ kind: 'resume' });
		expect(primaryAction({ status: 'active' }, [ev('charge', 0)])).toEqual({
			kind: 'mark',
			landmark: 'drying_end'
		});
		expect(primaryAction({ status: 'active' }, [ev('charge', 0), ev('drop', 600)])).toEqual({
			kind: 'complete'
		});
	});
});
