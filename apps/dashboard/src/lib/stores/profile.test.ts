import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// --- fetch mock (must be before module imports that use fetch) ---

const mockFetch = vi.fn<typeof fetch>();
vi.stubGlobal('fetch', mockFetch);

import {
	profileState,
	loadProfile,
	unloadProfile,
	startFollowing,
	stopFollowing,
	interpolateTarget,
	shouldSendSetpoint
} from './profile.svelte.js';
import type { ProfilePoint } from '#lib/api/profiles.js';

function jsonResponse(data: unknown, status = 200): Response {
	return new Response(JSON.stringify(data), {
		status,
		headers: { 'Content-Type': 'application/json' }
	});
}

function makePoint(time_seconds: number, target_temp: number): ProfilePoint {
	return {
		id: String(time_seconds),
		time_seconds,
		target_temp,
		fan_speed: null,
		notes: null,
		target_env_temp: null
	};
}

// --- interpolateTarget tests ---

describe('interpolateTarget', () => {
	const points: ProfilePoint[] = [makePoint(0, 150), makePoint(60, 200), makePoint(120, 220)];

	it('returns null for empty points array', () => {
		expect(interpolateTarget([], 30000, 0)).toBeNull();
	});

	it('holds first point temperature before first point', () => {
		expect(interpolateTarget(points, -10000, 0)).toBe(150);
	});

	it('returns first point temperature at exactly first point', () => {
		expect(interpolateTarget(points, 0, 0)).toBe(150);
	});

	it('holds last point temperature after last point', () => {
		expect(interpolateTarget(points, 180000, 0)).toBe(220);
	});

	it('returns last point temperature at exactly last point', () => {
		expect(interpolateTarget(points, 120000, 0)).toBe(220);
	});

	it('interpolates linearly between two points', () => {
		// Midway between 0s (150°C) and 60s (200°C) → 175°C
		expect(interpolateTarget(points, 30000, 0)).toBe(175);
		// Midway between 60s (200°C) and 120s (220°C) → 210°C
		expect(interpolateTarget(points, 90000, 0)).toBe(210);
	});

	it('returns exact temperature at exact point match', () => {
		expect(interpolateTarget(points, 0, 0)).toBe(150);
		expect(interpolateTarget(points, 60000, 0)).toBe(200);
		expect(interpolateTarget(points, 120000, 0)).toBe(220);
	});

	it('handles single-point profile', () => {
		const single = [makePoint(60, 200)];
		expect(interpolateTarget(single, 0, 0)).toBe(200);
		expect(interpolateTarget(single, 60000, 0)).toBe(200);
		expect(interpolateTarget(single, 120000, 0)).toBe(200);
	});

	it('applies lookahead offset correctly', () => {
		// At t=0ms with 30s lookahead → target at t=30s → 175°C
		expect(interpolateTarget(points, 0, 30000)).toBe(175);
	});

	it('handles unsorted points by sorting them', () => {
		const unsorted: ProfilePoint[] = [makePoint(120, 220), makePoint(0, 150), makePoint(60, 200)];
		expect(interpolateTarget(unsorted, 30000, 0)).toBe(175);
	});
});

// --- Profile store state tests ---

describe('profile store', () => {
	beforeEach(() => {
		mockFetch.mockReset();
	});

	afterEach(() => {
		unloadProfile();
	});

	it('loadProfile sets activeProfile and resets following state', async () => {
		const profile = {
			id: 'p1',
			name: 'Test Profile',
			description: null,
			target_total_time: 600,
			target_end_temp: 220,
			charge_temp: 150,
			created_at: '2026-01-01',
			points: [makePoint(0, 150)]
		};
		mockFetch.mockResolvedValueOnce(jsonResponse(profile));

		await loadProfile('p1');

		expect(profileState.activeProfile).toEqual(profile);
		expect(profileState.isFollowing).toBe(false);
		expect(profileState.lastSentSetpoint).toBeNull();
	});

	it('unloadProfile clears all state', async () => {
		const profile = {
			id: 'p1',
			name: 'Test',
			description: null,
			target_total_time: null,
			target_end_temp: null,
			charge_temp: null,
			created_at: '2026-01-01',
			points: [makePoint(0, 150)]
		};
		mockFetch.mockResolvedValueOnce(jsonResponse(profile));
		await loadProfile('p1');

		unloadProfile();

		expect(profileState.activeProfile).toBeNull();
		expect(profileState.isFollowing).toBe(false);
		expect(profileState.lastSentSetpoint).toBeNull();
	});

	it('startFollowing sets isFollowing when profile is loaded', async () => {
		const profile = {
			id: 'p1',
			name: 'Test',
			description: null,
			target_total_time: null,
			target_end_temp: null,
			charge_temp: null,
			created_at: '2026-01-01',
			points: [makePoint(0, 150)]
		};
		mockFetch.mockResolvedValueOnce(jsonResponse(profile));
		await loadProfile('p1');

		startFollowing();

		expect(profileState.isFollowing).toBe(true);
	});

	it('startFollowing does nothing when no profile is loaded', () => {
		startFollowing();
		expect(profileState.isFollowing).toBe(false);
	});

	it('stopFollowing clears isFollowing and lastSentSetpoint', async () => {
		const profile = {
			id: 'p1',
			name: 'Test',
			description: null,
			target_total_time: null,
			target_end_temp: null,
			charge_temp: null,
			created_at: '2026-01-01',
			points: [makePoint(0, 150)]
		};
		mockFetch.mockResolvedValueOnce(jsonResponse(profile));
		await loadProfile('p1');
		startFollowing();
		profileState.lastSentSetpoint = 200;

		stopFollowing();

		expect(profileState.isFollowing).toBe(false);
		expect(profileState.lastSentSetpoint).toBeNull();
	});
});

// --- Hysteresis tests ---

describe('shouldSendSetpoint (hysteresis)', () => {
	it('sends when lastSent is null (first setpoint)', () => {
		expect(shouldSendSetpoint(200, null)).toBe(true);
	});

	it('sends when delta >= threshold', () => {
		expect(shouldSendSetpoint(201, 200, 1.0)).toBe(true);
		expect(shouldSendSetpoint(199, 200, 1.0)).toBe(true);
	});

	it('does NOT send when delta < threshold', () => {
		expect(shouldSendSetpoint(200.5, 200, 1.0)).toBe(false);
		expect(shouldSendSetpoint(199.5, 200, 1.0)).toBe(false);
	});

	it('sends at exact threshold boundary', () => {
		expect(shouldSendSetpoint(201, 200, 1.0)).toBe(true);
		expect(shouldSendSetpoint(199, 200, 1.0)).toBe(true);
	});

	it('does NOT send at just below threshold', () => {
		expect(shouldSendSetpoint(200.99, 200, 1.0)).toBe(false);
		expect(shouldSendSetpoint(199.01, 200, 1.0)).toBe(false);
	});

	it('respects custom threshold', () => {
		expect(shouldSendSetpoint(202, 200, 3.0)).toBe(false);
		expect(shouldSendSetpoint(203, 200, 3.0)).toBe(true);
	});
});
