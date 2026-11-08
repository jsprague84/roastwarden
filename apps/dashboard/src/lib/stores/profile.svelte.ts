import { profiles, type ProfileWithPoints, type ProfilePoint } from '#lib/api/profiles.js';

// --- Reactive state (Svelte 5 runes) ---

export const profileState = $state<{
	activeProfile: ProfileWithPoints | null;
	isFollowing: boolean;
	lastSentSetpoint: number | null;
}>({
	activeProfile: null,
	isFollowing: false,
	lastSentSetpoint: null
});

// --- Actions ---

export async function loadProfile(id: string) {
	const profile = await profiles.get(id);
	profileState.activeProfile = profile;
	profileState.isFollowing = false;
	profileState.lastSentSetpoint = null;
}

/** Show an already fetched profile (the roast store decides which). */
export function setActiveProfile(profile: ProfileWithPoints) {
	profileState.activeProfile = profile;
	profileState.isFollowing = false;
	profileState.lastSentSetpoint = null;
}

export function unloadProfile() {
	profileState.activeProfile = null;
	profileState.isFollowing = false;
	profileState.lastSentSetpoint = null;
}

export function startFollowing() {
	if (!profileState.activeProfile) return;
	profileState.isFollowing = true;
}

export function stopFollowing() {
	profileState.isFollowing = false;
	profileState.lastSentSetpoint = null;
}

// --- Pure hysteresis check (exported for unit testing) ---

/**
 * Determines whether a new setpoint should be sent based on hysteresis threshold.
 * @param newTarget - The new interpolated target temperature
 * @param lastSent - The last setpoint that was sent (null if none sent yet)
 * @param threshold - Minimum delta to trigger a new send (default 1.0°C)
 */
export function shouldSendSetpoint(
	newTarget: number,
	lastSent: number | null,
	threshold = 1.0
): boolean {
	return lastSent === null || Math.abs(newTarget - lastSent) >= threshold;
}

// --- Pure interpolation function (exported for unit testing) ---

/**
 * Linear interpolation of target temperature from sorted profile points.
 * @param points - Profile points sorted by time_seconds
 * @param elapsedMs - Current elapsed time in milliseconds
 * @param lookaheadMs - Lookahead offset in milliseconds
 * @returns Target temperature at (elapsedMs + lookaheadMs)
 */
export function interpolateTarget(
	points: ProfilePoint[],
	elapsedMs: number,
	lookaheadMs: number
): number | null {
	if (points.length === 0) return null;

	const sorted = [...points].sort((a, b) => a.time_seconds - b.time_seconds);
	const targetMs = elapsedMs + lookaheadMs;
	const targetSec = targetMs / 1000;

	// Before first point: hold first temperature
	if (targetSec <= sorted[0].time_seconds) {
		return sorted[0].target_temp;
	}

	// After last point: hold last temperature
	if (targetSec >= sorted[sorted.length - 1].time_seconds) {
		return sorted[sorted.length - 1].target_temp;
	}

	// Find the two surrounding points and interpolate
	for (let i = 0; i < sorted.length - 1; i++) {
		const p1 = sorted[i];
		const p2 = sorted[i + 1];
		if (targetSec >= p1.time_seconds && targetSec <= p2.time_seconds) {
			const ratio = (targetSec - p1.time_seconds) / (p2.time_seconds - p1.time_seconds);
			return p1.target_temp + ratio * (p2.target_temp - p1.target_temp);
		}
	}

	return null;
}
