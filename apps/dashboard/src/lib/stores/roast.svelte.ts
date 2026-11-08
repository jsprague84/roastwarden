/**
 * The roast in progress, shared by every page.
 *
 * One place owns the selected roaster, its running session, the session's
 * landmarks and the profile the scope overlays. It lives at module level, so
 * leaving the Roast page (to the simulator, a profile, a setting) and coming
 * back loses nothing. The profile is derived from the session's
 * `profile_id` once a roast runs, and from the setup picker before that
 * (see lib/roast/live.ts for the rules).
 */
import { claimRoaster, track } from '#lib/stores/demo.svelte.js';
import { get } from 'svelte/store';
import { control } from '#lib/api/control.js';
import { events as eventsApi, type RoastEvent } from '#lib/api/events.js';
import { AlreadyMarkedError } from '#lib/api/http.js';
import { profiles } from '#lib/api/profiles.js';
import { roasterSetup, type RoasterSetup, type ControlLevel } from '#lib/api/roasters.js';
import { sessions, type RoastSession, type CreateSessionRequest } from '#lib/api/sessions.js';
import { virtualRoaster } from '#lib/api/virtual.js';
import { listDevices } from '#lib/api/devices.js';
import type { ConfiguredDevice } from '#lib/api/devices.js';
import {
	setSelectedDevice,
	clearHistory,
	seedHistory,
	telemetry,
	telemetryHistory,
	replayState,
	sessionEventsChanged,
	connectionStatus
} from './telemetry.js';
import { profileState, setActiveProfile, unloadProfile } from './profile.svelte.js';
import { notifications } from './notifications.js';
import {
	pickActiveSession,
	wantedProfileId,
	elapsedSinceStart,
	isVirtual,
	simLoaded
} from '#lib/roast/live.js';
import { allows, effectiveControl, effectiveOutputs } from '#lib/roaster/setup.js';

const DEVICE_KEY = 'roastwarden.roaster';
/** Key the old dashboard device picker used (sessionStorage); read once to carry the choice over. */
const LEGACY_DEVICE_KEY = 'roastwarden-selected-device';
const PREVIEW_KEY = 'roastwarden.nextProfile';
const POLL_MS = 10_000;

export const roast = $state<{
	/** Roasters to choose from: active first, then discovered (pending). */
	devices: ConfiguredDevice[];
	devicesLoaded: boolean;
	/** The selected roaster's `device_id` (the MQTT id, not the row uuid). */
	deviceId: string | null;
	/** The roast running on that roaster (active or paused), if any. */
	session: RoastSession | null;
	/** Its landmarks, in time order. */
	events: RoastEvent[];
	/** True once `events` belongs to `session`. */
	eventsLoaded: boolean;
	/** Profile picked for the next roast (shown as a preview before Start). */
	previewProfileId: string | null;
	/** The roast completed last in this browser, for the "review" step. */
	lastCompleted: RoastSession | null;
	/** A start/complete/mark request is in flight. */
	busy: boolean;
	/** The selected roaster's setup (docs/roaster-setup.md); null until it has loaded. */
	setup: RoasterSetup | null;
}>({
	devices: [],
	devicesLoaded: false,
	deviceId: null,
	session: null,
	events: [],
	eventsLoaded: false,
	previewProfileId: null,
	lastCompleted: null,
	busy: false,
	setup: null
});

function readStorage(key: string, storage: () => Storage | undefined): string | null {
	try {
		return storage()?.getItem(key) ?? null;
	} catch {
		return null;
	}
}
function writeStorage(key: string, value: string | null) {
	try {
		if (value == null) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
	} catch {
		/* storage unavailable */
	}
}
const local = () => (typeof localStorage === 'undefined' ? undefined : localStorage);
const sessionStore = () => (typeof sessionStorage === 'undefined' ? undefined : sessionStorage);

// ---------------------------------------------------------------- devices --

export async function refreshDevices() {
	try {
		const all = await listDevices();
		const rank = (d: ConfiguredDevice) =>
			d.status === 'active' ? 0 : d.status === 'pending' ? 1 : 2;
		roast.devices = all
			.filter((d) => d.status === 'active' || d.status === 'pending')
			.sort((a, b) => rank(a) - rank(b));
	} catch {
		/* keep what we have */
	}
	roast.devicesLoaded = true;
}

/** Pick the roaster every page works with. */
export function selectDevice(id: string | null) {
	if (id === roast.deviceId) return;
	roast.deviceId = id;
	writeStorage(DEVICE_KEY, id);
	setSelectedDevice(id);
	roast.session = null;
	setEvents([]);
	roast.eventsLoaded = false;
	roast.setup = null;
	syncProfile();
	void refreshSession();
	void refreshSetup();
}

export const selectedDevice = () =>
	roast.devices.find((d) => d.device_id === roast.deviceId) ?? null;

// ------------------------------------------------------------------ setup --

/** Re-read the selected roaster's setup. */
async function refreshSetup() {
	const dev = roast.deviceId;
	if (!dev) return;
	try {
		const s = await roasterSetup.get(dev);
		if (dev === roast.deviceId) roast.setup = s;
	} catch {
		/* keep what we have (until it loads, the page acts as before; the server gates) */
	}
}

/** A setup was saved (Roasters → a roaster → Control): the Roast page follows at once. */
export function setupSaved(s: RoasterSetup) {
	if (s.device_id === roast.deviceId) roast.setup = s;
}

/** What Roastwarden may send the selected roaster (docs/roaster-setup.md). */
export const controlLevel = (): ControlLevel => effectiveControl(roast.setup, roast.deviceId);

/** The selected roaster's outputs and fan floor (docs/roaster-setup.md, "Outputs"). */
export const roasterOutputs = () => effectiveOutputs(roast.setup, roast.deviceId);

// ---------------------------------------------------------------- profile --

let profileToken = 0;
let profileLoading: string | null = null;

/** Make the overlay match the session (or the setup picker). */
function syncProfile() {
	const wanted = wantedProfileId(roast.session, roast.previewProfileId);
	const have = profileState.activeProfile?.id ?? null;
	if (wanted === have) {
		profileLoading = null;
		return;
	}
	if (!wanted) {
		profileToken++;
		profileLoading = null;
		unloadProfile();
		return;
	}
	if (profileLoading === wanted) return;
	profileLoading = wanted;
	const token = ++profileToken;
	profiles
		.get(wanted)
		.then((p) => {
			if (token !== profileToken) return;
			setActiveProfile(p);
		})
		.catch(() => {
			/* the next poll retries */
		})
		.finally(() => {
			if (token === profileToken) profileLoading = null;
		});
}

/** Choose the profile for the next roast ('' or null: none). */
export function setPreviewProfile(id: string | null) {
	roast.previewProfileId = id || null;
	writeStorage(PREVIEW_KEY, roast.previewProfileId);
	syncProfile();
}

// ---------------------------------------------------------------- session --

let refreshing = false;
let refreshAgain = false;

/** Re-read which roast runs on the selected roaster. */
export async function refreshSession() {
	if (refreshing) {
		refreshAgain = true;
		return;
	}
	refreshing = true;
	try {
		do {
			refreshAgain = false;
			const dev = roast.deviceId;
			if (!dev) {
				adoptSession(null);
				continue;
			}
			try {
				const list = await sessions.list(dev, 10);
				if (dev !== roast.deviceId) continue; // the roaster changed meanwhile
				adoptSession(pickActiveSession(list, dev));
			} catch {
				/* server unreachable: keep what we have */
			}
		} while (refreshAgain);
	} finally {
		refreshing = false;
	}
	syncProfile();
}

function adoptSession(s: RoastSession | null) {
	const had = roast.session;
	if (s?.id !== had?.id) {
		setEvents([]);
		roast.eventsLoaded = false;
		roast.session = s;
		if (s) {
			void refreshEvents();
			void seedFromServer(s);
		}
	} else if (
		s &&
		had &&
		(s.status !== had.status || s.profile_id !== had.profile_id || s.name !== had.name)
	) {
		roast.session = s;
	}
	syncProfile();
}

/** Bumped on every local change to `roast.events`. */
let eventsVersion = 0;
function setEvents(list: RoastEvent[]) {
	eventsVersion++;
	roast.events = list.slice().sort((a, b) => a.elapsed_seconds - b.elapsed_seconds);
}

/** Reload the running roast's landmarks. */
export async function refreshEvents() {
	const s = roast.session;
	if (!s) return;
	const version = eventsVersion;
	try {
		const list = await eventsApi.list(s.id);
		if (roast.session?.id !== s.id) return;
		if (version !== eventsVersion) {
			// A mark landed while this request was out: its answer may predate
			// it. Ask again rather than overwrite the newer list.
			void refreshEvents();
			return;
		}
		setEvents(list);
	} catch {
		/* keep */
	}
	if (roast.session?.id === s.id) roast.eventsLoaded = true;
}

// eslint-disable-next-line svelte/prefer-svelte-reactivity -- which sessions were seeded once; nothing renders it
const seeded = new Set<string>();

/**
 * After a reload mid-roast the browser's chart history is empty: restore it
 * from what the server recorded for this session.
 */
async function seedFromServer(s: RoastSession) {
	if (!s.start_time || seeded.has(s.id)) return;
	seeded.add(s.id);
	const startMs = new Date(s.start_time).getTime();
	const first = get(telemetryHistory)[0]?.timestamp;
	if (first != null && first <= startMs) return; // the browser already holds the whole roast
	try {
		const full = await sessions.get(s.id);
		seedHistory(
			(full.telemetry ?? [])
				.filter((t) => t.bean_temp != null && t.env_temp != null)
				.map((t) => ({
					timestamp: startMs + t.elapsed_seconds * 1000,
					telemetry: {
						beanTemp: t.bean_temp as number,
						envTemp: t.env_temp as number,
						heater: t.heater ?? 0,
						fan: t.fan ?? 0,
						setpoint: t.setpoint ?? 0,
						controlMode: 0,
						heaterEnable: 0,
						rateOfRise: t.rate_of_rise ?? undefined
					}
				}))
		);
	} catch {
		seeded.delete(s.id);
	}
}

async function run<T>(what: string, fn: () => Promise<T>): Promise<T | undefined> {
	roast.busy = true;
	try {
		return await fn();
	} catch (e) {
		notifications.add(`${what}: ${e instanceof Error ? e.message : String(e)}`, 'error');
		return undefined;
	} finally {
		roast.busy = false;
	}
}

export interface StartDetails {
	name?: string;
	/** A bean from stock (docs/batches.md). */
	bean_id?: string;
	/** The planned roast this starts. */
	planned_roast_id?: string;
	bean_origin?: string;
	bean_variety?: string;
	green_weight?: number;
	notes?: string;
}

/** Create and start a session on the selected roaster with the picked profile. */
export async function startRoast(details: StartDetails = {}) {
	const dev = roast.deviceId;
	if (!dev) return;
	await run('Failed to start the roast', async () => {
		const req: CreateSessionRequest = {
			...details,
			// eslint-disable-next-line svelte/prefer-svelte-reactivity -- a one-off timestamp for the name
			name: details.name?.trim() || `Roast ${new Date().toLocaleString()}`,
			device_id: dev,
			profile_id: roast.previewProfileId || undefined
		};
		const created = await sessions.create(req);
		const started = await sessions.start(created.id);
		track('Roast started');
		clearHistory();
		seeded.add(started.id);
		roast.lastCompleted = null;
		adoptSession(started);
	});
}

export async function pauseRoast() {
	const s = roast.session;
	if (!s) return;
	const r = await run('Failed to pause', () => sessions.pause(s.id));
	if (r) adoptSession(r);
}

export async function resumeRoast() {
	const s = roast.session;
	if (!s) return;
	const r = await run('Failed to resume', () => sessions.resume(s.id));
	if (r) adoptSession(r);
}

/** Finish the roast: heater off, session completed, ready for the next one. */
export async function completeRoast() {
	const s = roast.session;
	if (!s) return;
	const dev = roast.deviceId;
	const done = await run('Failed to complete the roast', async () => {
		if (dev && allows(controlLevel(), 'sliders') && roasterOutputs().outputs.heater)
			await control.setHeaterEnable(dev, false).catch(() => {});
		return sessions.complete(s.id);
	});
	if (done) {
		roast.lastCompleted = done;
		adoptSession(null);
	}
}

/**
 * Cancel the roast: the server stops the drive, cuts heater power, cools a hot chamber and
 * deletes the roast (docs/roast-sequence.md). Nothing is saved.
 */
const LANDMARK_LABEL: Record<string, string> = {
	turning_point: 'Turning point',
	drying_end: 'Dry end',
	first_crack_start: 'First crack',
	first_crack_end: 'First crack end',
	second_crack_start: 'Second crack',
	second_crack_end: 'Second crack end',
	cool_end: 'Cool end'
};

export async function cancelRoast() {
	const s = roast.session;
	if (!s) return;
	const dev = roast.deviceId;
	const done = await run('Failed to cancel the roast', async () => {
		await sessions.cancel(s.id);
		return true;
	});
	if (!done) return;
	if (isVirtual(dev)) {
		try {
			if (simLoaded(await virtualRoaster.get(dev ?? undefined), dev))
				await virtualRoaster.dump(dev ?? undefined);
		} catch {
			/* the simulator is optional */
		}
	}
	adoptSession(null);
	notifications.add('Roast cancelled: nothing was saved', 'success');
}

/**
 * Mark a landmark now. DROP also switches the heater off. On the virtual
 * roaster CHARGE loads the simulated beans and DROP dumps them, so the
 * simulator follows the same buttons as a real roast.
 */
export async function markLandmark(type: string) {
	const s = roast.session;
	if (!s || roast.events.some((e) => e.event_type === type)) return;
	const dev = roast.deviceId;
	const elapsed = Math.round(elapsedSinceStart(s, Date.now()) * 10) / 10;
	const bt = get(telemetry)?.beanTemp;
	let already = false;
	const created = await run('Failed to mark', async () => {
		try {
			return await eventsApi.create(s.id, {
				event_type: type,
				elapsed_seconds: elapsed,
				temperature: bt ?? undefined
			});
		} catch (e) {
			// The server marked it while this page was not listening (an auto DROP in a hidden
			// tab): take its marks rather than mark it twice.
			if (!(e instanceof AlreadyMarkedError)) throw e;
			already = true;
			return undefined;
		}
	});
	if (already) {
		notifications.add(
			`${LANDMARK_LABEL[type] ?? type} was already marked on this roast`,
			'warning',
			6000
		);
		await refreshEvents();
		return;
	}
	if (!created) return;
	if (roast.session?.id === s.id) {
		setEvents([...roast.events, created]);
	}
	// A mis-tap is one tap to take back. Not for CHARGE and DROP: the roast sequence has
	// already acted on the roaster, and removing the mark would not undo that.
	if (type !== 'charge' && type !== 'drop') {
		notifications.add(`Marked ${LANDMARK_LABEL[type] ?? type}`, 'success', 8000, {
			label: 'Undo',
			callback: () => void unmarkLandmark(created)
		});
	}
	if (
		type === 'drop' &&
		dev &&
		allows(controlLevel(), 'sliders') &&
		roasterOutputs().outputs.heater
	)
		await control.setHeaterEnable(dev, false).catch(() => {});
	if (isVirtual(dev) && (type === 'charge' || type === 'drop')) {
		try {
			const loaded = simLoaded(await virtualRoaster.get(dev ?? undefined), dev);
			if (type === 'charge' && !loaded) await virtualRoaster.charge(dev ?? undefined);
			if (type === 'drop' && loaded) await virtualRoaster.dump(dev ?? undefined);
		} catch (e) {
			notifications.add(`Simulator: ${e instanceof Error ? e.message : String(e)}`, 'error');
		}
	}
}

export async function unmarkLandmark(ev: RoastEvent) {
	const s = roast.session;
	if (!s) return;
	const ok = await run('Failed to remove the mark', async () => {
		await eventsApi.delete(s.id, ev.id);
		return true;
	});
	if (ok) setEvents(roast.events.filter((e) => e.id !== ev.id));
}

// ---------------------------------------------------------------- startup --

let started = false;

/** Start following the roast. Call once from the root layout. */
export function initRoast(): () => void {
	if (started) return () => {};
	started = true;
	roast.previewProfileId = readStorage(PREVIEW_KEY, local) || null;
	const stored = readStorage(DEVICE_KEY, local) ?? readStorage(LEGACY_DEVICE_KEY, sessionStore);

	// On a public demo the visitor's own simulated roaster comes first (made by this call).
	void claimRoaster()
		.then(async (own) => {
			await refreshDevices();
			return own;
		})
		.then((own) => {
			const pick =
				(own && roast.devices.some((d) => d.device_id === own) ? own : null) ??
				(stored && roast.devices.some((d) => d.device_id === stored) ? stored : null) ??
				roast.devices.find((d) => d.status === 'active')?.device_id ??
				roast.devices[0]?.device_id ??
				null;
			selectDevice(pick);
			syncProfile();
		});

	const timer = setInterval(() => {
		void refreshSession();
		void refreshSetup();
	}, POLL_MS);
	// A replay shows up for a session this browser does not know: someone
	// started a roast elsewhere.
	const unReplay = replayState.subscribe((r) => {
		if (r?.active && r.session_id !== roast.session?.id) void refreshSession();
	});
	// Landmarks marked on the server (auto events, alarm actions, another browser).
	const unEvents = sessionEventsChanged.subscribe((ch) => {
		if (ch && ch.session_id === roast.session?.id) void refreshEvents();
	});
	// Those notices only reach an open socket, and a hidden tab closes it: on the way back
	// (or after a dropped connection) read the marks again, or an auto DROP made meanwhile
	// leaves the DROP button up.
	let wasConnected = false;
	const unConn = connectionStatus.subscribe((st) => {
		if (st === 'connected' && wasConnected === false && roast.session) {
			void refreshSession();
			void refreshEvents();
		}
		wasConnected = st === 'connected';
	});
	return () => {
		clearInterval(timer);
		unReplay();
		unEvents();
		unConn();
		started = false;
	};
}
