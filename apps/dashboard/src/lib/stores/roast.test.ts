import { describe, it, expect, vi, beforeEach } from 'vitest';

// --- a tiny fake server behind fetch ---

type Json = unknown;
const state = {
	sessions: [] as Array<Record<string, Json>>,
	events: {} as Record<string, Array<Record<string, Json>>>,
	vr: {
		enabled: true,
		loaded: false,
		bt: 25,
		et: 25,
		heater_applied: 0,
		fan: 0,
		drum: {
			enabled: true,
			device_id: 'virtual_drum',
			loaded: false,
			bt: 25,
			exhaust: 25,
			inlet: 25,
			burner: 0,
			air: 0,
			drum: 0
		}
	},
	calls: [] as string[],
	/** When set, event list GETs answer with this snapshot once `release` is called. */
	hold: null as null | { snapshot: Array<Record<string, Json>>; release: Promise<void> }
};

function profile(id: string) {
	return { id, name: id, points: [{ id: 'p', time_seconds: 0, target_temp: 30 }], events: [] };
}

function reply(body: Json, status = 200) {
	return Promise.resolve(
		new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } })
	);
}

const mockFetch = vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
	const url = new URL(String(input), 'http://x');
	const path = url.pathname;
	const method = init?.method ?? 'GET';
	state.calls.push(`${method} ${path}`);
	if (path === '/api/devices') {
		return reply([
			{ id: 'u1', name: 'ESP32', device_id: 'esp32_roaster_01', status: 'active' },
			{ id: 'u2', name: 'Virtual', device_id: 'virtual_roaster', status: 'pending' }
		]);
	}
	if (path === '/api/sessions' && method === 'GET') {
		const dev = url.searchParams.get('device_id');
		return reply(state.sessions.filter((s) => !dev || s.device_id === dev));
	}
	if (path === '/api/sessions' && method === 'POST') {
		const req = JSON.parse(String(init?.body));
		const s = {
			id: `s${state.sessions.length + 1}`,
			status: 'created',
			start_time: null,
			created_at: new Date().toISOString(),
			...req
		};
		state.sessions.unshift(s);
		return reply(s);
	}
	let m = path.match(/^\/api\/sessions\/([^/]+)\/(start|complete)$/);
	if (m) {
		const s = state.sessions.find((x) => x.id === m![1])!;
		s.status = m[2] === 'start' ? 'active' : 'completed';
		if (m[2] === 'start') s.start_time = new Date().toISOString();
		return reply(s);
	}
	m = path.match(/^\/api\/sessions\/([^/]+)\/events$/);
	if (m) {
		const list = (state.events[m[1]] ??= []);
		if (method === 'POST') {
			const req = JSON.parse(String(init?.body));
			// The server's rule: one CHARGE and one DROP per roast.
			if (
				(req.event_type === 'charge' || req.event_type === 'drop') &&
				list.some((x) => x.event_type === req.event_type)
			) {
				const message = `${String(req.event_type).toUpperCase()} is already marked on this roast`;
				return reply({ success: false, error: { code: 'ALREADY_MARKED', message } }, 409);
			}
			const e = { id: `e${list.length + 1}`, session_id: m[1], ...req };
			list.push(e);
			return reply(e);
		}
		if (state.hold) {
			const h = state.hold;
			state.hold = null;
			return h.release.then(() => reply(h.snapshot));
		}
		return reply(list);
	}
	m = path.match(/^\/api\/sessions\/([^/]+)$/);
	if (m) return reply({ ...state.sessions.find((x) => x.id === m![1]), telemetry: [] });
	m = path.match(/^\/api\/profiles\/([^/]+)$/);
	if (m) return reply(profile(m[1]));
	if (path === '/api/virtual-roaster') return reply(state.vr);
	if (path === '/api/virtual-roaster/charge' || path === '/api/virtual-roaster/dump') {
		const loaded = path.endsWith('/charge');
		if (url.searchParams.get('roaster') === 'virtual_drum') state.vr.drum.loaded = loaded;
		else state.vr.loaded = loaded;
		return reply(state.vr);
	}
	if (path.startsWith('/api/roaster/')) return reply({ ok: true });
	return reply({}, 404);
});
vi.stubGlobal('fetch', mockFetch);

// The live socket, enough for telemetry.ts to open, close on a hidden tab and open again.
const sockets: FakeWS[] = [];
class FakeWS {
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSED = 3;
	readyState = FakeWS.CONNECTING;
	binaryType = 'blob';
	onopen: (() => void) | null = null;
	onmessage: ((ev: MessageEvent) => void) | null = null;
	onclose: (() => void) | null = null;
	onerror: (() => void) | null = null;
	constructor() {
		sockets.push(this);
	}
	send() {}
	close() {
		this.readyState = FakeWS.CLOSED;
	}
	open() {
		this.readyState = FakeWS.OPEN;
		this.onopen?.();
	}
}
vi.stubGlobal('WebSocket', FakeWS);
const { initTelemetrySocket, destroyTelemetrySocket } = await import('./telemetry.js');

const {
	roast,
	initRoast,
	selectDevice,
	setPreviewProfile,
	startRoast,
	markLandmark,
	completeRoast,
	refreshSession,
	refreshEvents
} = await import('./roast.svelte.js');
const { profileState, unloadProfile } = await import('./profile.svelte.js');

const settle = async () => {
	for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

describe('roast store', () => {
	let stop: () => void = () => {};

	beforeEach(async () => {
		stop();
		state.sessions = [];
		state.events = {};
		state.vr.loaded = false;
		state.vr.drum.loaded = false;
		state.calls = [];
		localStorage.clear();
		sessionStorage.clear();
		unloadProfile();
		roast.previewProfileId = null;
		roast.session = null;
		roast.events = [];
		roast.deviceId = null;
	});

	it('restores the roaster picked by the old dashboard and lists discovered roasters', async () => {
		sessionStorage.setItem('roastwarden-selected-device', 'virtual_roaster');
		stop = initRoast();
		await settle();
		expect(roast.deviceId).toBe('virtual_roaster');
		expect(roast.devices.map((d) => d.device_id)).toEqual(['esp32_roaster_01', 'virtual_roaster']);
		expect(localStorage.getItem('roastwarden.roaster')).toBe('virtual_roaster');
	});

	it('keeps the profile overlay through a roast and derives it from the session', async () => {
		stop = initRoast();
		await settle();
		selectDevice('virtual_roaster');
		setPreviewProfile('fluidbed-espresso');
		await settle();
		expect(profileState.activeProfile?.id).toBe('fluidbed-espresso');

		await startRoast();
		await settle();
		expect(roast.session?.profile_id).toBe('fluidbed-espresso');
		expect(profileState.activeProfile?.id).toBe('fluidbed-espresso');

		// What used to clear it: leaving the page and coming back, a poll.
		await refreshSession();
		await settle();
		expect(profileState.activeProfile?.id).toBe('fluidbed-espresso');

		// Changing the picker mid-roast does not change the roast's overlay.
		setPreviewProfile('fluidbed-light');
		await settle();
		expect(profileState.activeProfile?.id).toBe('fluidbed-espresso');
	});

	it('ignores a roast running on another roaster', async () => {
		state.sessions = [
			{
				id: 'real',
				device_id: 'esp32_roaster_01',
				profile_id: null,
				status: 'active',
				start_time: new Date().toISOString(),
				created_at: new Date().toISOString()
			}
		];
		stop = initRoast();
		await settle();
		selectDevice('virtual_roaster');
		setPreviewProfile('fluidbed-espresso');
		await refreshSession();
		await settle();
		expect(roast.session).toBeNull();
		expect(profileState.activeProfile?.id).toBe('fluidbed-espresso');
		selectDevice('esp32_roaster_01');
		await settle();
		expect(roast.session?.id).toBe('real');
		// That roast has no profile: no overlay, whatever the picker says.
		expect(profileState.activeProfile).toBeNull();
	});

	it('charges and dumps the simulator with CHARGE and DROP', async () => {
		stop = initRoast();
		await settle();
		selectDevice('virtual_roaster');
		await startRoast();
		await settle();
		await markLandmark('charge');
		expect(state.vr.loaded).toBe(true);
		expect(roast.events.map((e) => e.event_type)).toEqual(['charge']);
		await markLandmark('drop');
		expect(state.vr.loaded).toBe(false);
		expect(state.calls).toContain('POST /api/roaster/virtual_roaster/control/heater_enable');
		await completeRoast();
		expect(roast.session).toBeNull();
		expect(roast.lastCompleted?.status).toBe('completed');
	});

	it('charges and dumps the simulated drum, not the fluid bed, on virtual_drum', async () => {
		stop = initRoast();
		await settle();
		selectDevice('virtual_drum');
		await startRoast();
		await settle();
		await markLandmark('charge');
		expect(state.vr.drum.loaded).toBe(true);
		expect(state.vr.loaded).toBe(false);
		expect(state.calls).toContain('POST /api/virtual-roaster/charge');
		await markLandmark('drop');
		expect(state.vr.drum.loaded).toBe(false);
	});

	it('does not let a slow reload undo a mark made meanwhile', async () => {
		stop = initRoast();
		await settle();
		selectDevice('virtual_roaster');
		await startRoast();
		await settle();
		let release!: () => void;
		state.hold = { snapshot: [], release: new Promise<void>((r) => (release = r)) };
		const reload = refreshEvents(); // answers with the list from before the mark
		await markLandmark('charge');
		release();
		await reload;
		await settle();
		expect(roast.events.map((e) => e.event_type)).toEqual(['charge']);
	});

	// Seen on a real roast: auto DROP fired while the Roast page's tab was hidden.
	it("takes the server's DROP instead of marking a second one", async () => {
		stop = initRoast();
		await settle();
		selectDevice('virtual_roaster');
		await startRoast();
		await settle();
		await markLandmark('charge');
		const sid = roast.session!.id;
		// Auto DROP on the server; this page never heard of it
		state.events[sid].push({
			id: 'auto',
			session_id: sid,
			event_type: 'drop',
			elapsed_seconds: 625
		});
		state.calls = [];
		await markLandmark('drop');
		await settle();
		expect(state.events[sid].filter((e) => e.event_type === 'drop')).toHaveLength(1);
		expect(roast.events.find((e) => e.event_type === 'drop')?.id).toBe('auto');
		// Nothing acted on the roaster a second time
		expect(state.calls.filter((c) => c.includes('/api/roaster/') || c.includes('/dump'))).toEqual(
			[]
		);
	});

	it('reads the marks again when a hidden tab comes back', async () => {
		stop = initRoast();
		initTelemetrySocket();
		try {
			sockets[sockets.length - 1].open();
			await settle();
			selectDevice('virtual_roaster');
			await startRoast();
			await settle();
			await markLandmark('charge');
			const sid = roast.session!.id;
			Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
			document.dispatchEvent(new Event('visibilitychange'));
			// Auto DROP while hidden: its notice went to no socket
			state.events[sid].push({
				id: 'auto',
				session_id: sid,
				event_type: 'drop',
				elapsed_seconds: 625
			});
			Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
			document.dispatchEvent(new Event('visibilitychange'));
			sockets[sockets.length - 1].open();
			await settle();
			expect(roast.events.map((e) => e.event_type)).toEqual(['charge', 'drop']);
		} finally {
			destroyTelemetrySocket();
		}
	});
});
