import { describe, it, expect } from 'vitest';
import {
	pickActiveSession,
	wantedProfileId,
	roastStage,
	nextLandmark,
	sinceCharge,
	elapsedSinceStart,
	isVirtual,
	isVirtualDrum,
	simLoaded,
	fcDue
} from './live.js';
import type { RoastSession } from '#lib/api/sessions.js';

function session(p: Partial<RoastSession>): RoastSession {
	return {
		id: 's',
		name: 'Roast',
		device_id: 'esp32_roaster_01',
		profile_id: null,
		status: 'active',
		start_time: '2026-10-22T11:00:00Z',
		end_time: null,
		created_at: '2026-10-22T11:00:00Z',
		bean_origin: null,
		bean_variety: null,
		green_weight: null,
		roasted_weight: null,
		notes: null,
		total_time_seconds: null,
		max_temp: null,
		first_crack_time: null,
		development_time_ratio: null,
		weight_loss_pct: null,
		max_ror: null,
		avg_ror_drying: null,
		avg_ror_maillard: null,
		avg_ror_development: null,
		drying_end_time: null,
		drying_end_temp: null,
		auc_value: null,
		...p
	} as RoastSession;
}

const ev = (event_type: string, elapsed_seconds = 0) => ({ event_type, elapsed_seconds });

describe('pickActiveSession', () => {
	it('only takes sessions on the selected roaster', () => {
		const list = [
			session({ id: 'real', device_id: 'esp32_roaster_01', start_time: '2026-10-22T11:10:00Z' }),
			session({
				id: 'sim',
				device_id: 'virtual_roaster',
				profile_id: 'fluidbed-espresso',
				start_time: '2026-10-22T11:08:00Z'
			})
		];
		expect(pickActiveSession(list, 'virtual_roaster')?.id).toBe('sim');
		expect(pickActiveSession(list, 'esp32_roaster_01')?.id).toBe('real');
		expect(pickActiveSession(list, null)).toBeNull();
	});

	it('ignores completed sessions and prefers the newest live one', () => {
		const list = [
			session({ id: 'old', status: 'paused', start_time: '2026-10-22T10:00:00Z' }),
			session({ id: 'done', status: 'completed', start_time: '2026-10-22T12:00:00Z' }),
			session({ id: 'new', status: 'active', start_time: '2026-10-22T11:00:00Z' })
		];
		expect(pickActiveSession(list, 'esp32_roaster_01')?.id).toBe('new');
		expect(pickActiveSession([list[1]], 'esp32_roaster_01')).toBeNull();
	});
});

describe('wantedProfileId', () => {
	it('uses the session profile once a roast runs, even without one', () => {
		expect(wantedProfileId(session({ profile_id: 'fluidbed-light' }), 'fluidbed-medium')).toBe(
			'fluidbed-light'
		);
		expect(wantedProfileId(session({ profile_id: null }), 'fluidbed-medium')).toBeNull();
	});

	it('uses the picked preview before a roast', () => {
		expect(wantedProfileId(null, 'fluidbed-medium')).toBe('fluidbed-medium');
		expect(wantedProfileId(null, '')).toBeNull();
		expect(wantedProfileId(null, null)).toBeNull();
	});
});

describe('roastStage', () => {
	it('walks setup → charge → roasting → dropped', () => {
		expect(roastStage(null, [])).toBe('setup');
		expect(roastStage(session({ status: 'completed' }), [])).toBe('setup');
		expect(roastStage(session({}), [])).toBe('charge');
		// An auto turning point before CHARGE does not count as charged.
		expect(roastStage(session({}), [ev('turning_point')])).toBe('charge');
		expect(roastStage(session({ status: 'paused' }), [ev('charge')])).toBe('roasting');
		expect(roastStage(session({}), [ev('charge'), ev('drop')])).toBe('dropped');
	});
});

describe('nextLandmark', () => {
	it('follows DRY → FC → FC end → DROP', () => {
		expect(nextLandmark([ev('charge')])).toBe('drying_end');
		expect(nextLandmark([ev('charge'), ev('drying_end')])).toBe('first_crack_start');
		expect(nextLandmark([ev('charge'), ev('drying_end'), ev('first_crack_start')])).toBe(
			'first_crack_end'
		);
		expect(
			nextLandmark([ev('charge'), ev('drying_end'), ev('first_crack_start'), ev('first_crack_end')])
		).toBe('drop');
	});

	it('does not send you back for a missed DRY', () => {
		expect(nextLandmark([ev('charge'), ev('first_crack_start')])).toBe('first_crack_end');
	});
});

describe('time', () => {
	const s = session({ start_time: '2026-10-22T11:00:00Z' });
	const t0 = new Date('2026-10-22T11:00:00Z').getTime();

	it('counts from the session start and from CHARGE', () => {
		expect(elapsedSinceStart(s, t0 + 90_000)).toBe(90);
		expect(sinceCharge(s, [], t0 + 90_000)).toBeNull();
		expect(sinceCharge(s, [ev('charge', 39)], t0 + 90_000)).toBe(51);
	});

	it('knows the virtual roaster and the simulated drum', () => {
		expect(isVirtual('virtual_roaster')).toBe(true);
		expect(isVirtual('virtual_drum')).toBe(true);
		expect(isVirtual('esp32_roaster_01')).toBe(false);
		expect(isVirtualDrum('virtual_drum')).toBe(true);
		expect(isVirtualDrum('virtual_roaster')).toBe(false);
	});

	it('reads the right simulator for beans loaded', () => {
		const vr = { loaded: true, drum: { loaded: false } as never };
		expect(simLoaded(vr, 'virtual_roaster')).toBe(true);
		expect(simLoaded(vr, 'virtual_drum')).toBe(false);
		expect(simLoaded({ loaded: false }, 'virtual_drum')).toBe(false);
	});
});

describe('fcDue', () => {
	it('goes by BT near the target while BT rises', () => {
		expect(fcDue(194.9, 9, 197)).toBe(false);
		expect(fcDue(195.0, 9, 197)).toBe(true);
		expect(fcDue(196, 0, 197)).toBe(false);
		expect(fcDue(194.2, 9, null)).toBe(true);
		expect(fcDue(null, 9, 197)).toBe(false);
	});
});
