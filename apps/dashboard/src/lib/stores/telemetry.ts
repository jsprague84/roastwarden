import { writable, derived, readonly } from 'svelte/store';
import type { Telemetry, TelemetryMessage, ConnectionStatus } from '#lib/api/telemetry.js';
import type { ReplayState } from '#lib/api/replay.js';
import { settings } from '#lib/api/settings.js';
import { Decoder } from 'cbor-x';
import { sampleToTelemetry, hasTemperature, type SamplesFrame } from '#lib/telemetry/sample.js';

/** A server-side roast alarm that fired (mirrors the backend AlarmEvent). */
export interface AlarmEvent {
	device_id: string;
	session_id: string;
	session_name: string;
	name: string;
	condition_type: string;
	threshold: number;
	message: string;
	value: number | null;
	elapsed_seconds: number;
	fired_at: string;
	/** What the alarm did on the roaster, one line per action. */
	actions?: string[];
	/** When this browser heard it (ms, its own clock): how old it is, whatever the server's clock says. */
	received_at?: number;
}

function getWsUrl(): string {
	if (import.meta.env.VITE_WS_URL) return import.meta.env.VITE_WS_URL;
	if (typeof window === 'undefined') return 'ws://localhost:8080/ws/v2/telemetry';
	const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
	return `${proto}//${window.location.host}/ws/v2/telemetry`;
}
const WS_URL = getWsUrl();

const MAX_BACKOFF_MS = 30_000;
/** In-memory history kept in the browser: two hours at 1 Hz, same as the server ring. */
const MAX_HISTORY = 7200;
// Timestamps are 64-bit integers on the wire; decode them as JS numbers (safe below 2^53).
// `int64AsNumber` is honoured at runtime but missing from cbor-x's Options typing.
const cbor = new Decoder({
	mapsAsObjects: true,
	useRecords: false,
	int64AsNumber: true
} as ConstructorParameters<typeof Decoder>[0]);

/** Internal writable stores. */
const _telemetry = writable<Telemetry | null>(null);
const _deviceId = writable<string | null>(null);
const _connectionStatus = writable<ConnectionStatus>('disconnected');
const _history = writable<{ timestamp: number; telemetry: Telemetry }[]>([]);
const _replayState = writable<ReplayState | null>(null);
const _alarmEvent = writable<AlarmEvent | null>(null);
const _sessionEventsChanged = writable<{ session_id: string; at: number } | null>(null);
const _historyGap = writable(false);

/** Public read-only stores. */
export const telemetry = readonly(_telemetry);
export const deviceId = readonly(_deviceId);
export const connectionStatus = readonly(_connectionStatus);
export const telemetryHistory = readonly(_history);
/** Latest server-side replay state for the selected device. */
export const replayState = readonly(_replayState);
/** Most recent server-side alarm for the selected device. */
export const alarmEvent = readonly(_alarmEvent);
/** A session's landmarks changed on the server (auto event, alarm, another browser): reload them. */
export const sessionEventsChanged = readonly(_sessionEventsChanged);
/** True when the server could not fill a reconnect gap (history has a hole). */
export const historyGap = readonly(_historyGap);

/** Seed the replay state (e.g. after fetching it over HTTP on page load). */
export function setReplayState(state: ReplayState | null) {
	_replayState.set(state);
}

/** RoR configuration stores. */
const _rorWindowMs = writable(30_000);
const _rorAlgorithm = writable<string>('moving_average');

/** Update RoR configuration. */
export function setRorConfig(windowSeconds: number, algorithm: string) {
	_rorWindowMs.set(windowSeconds * 1000);
	_rorAlgorithm.set(algorithm);
}

/** Load RoR settings from the server. */
export async function loadRorSettings() {
	try {
		const s = await settings.get();
		const windowSec = parseInt(s['ror_window_seconds'] ?? '30', 10);
		const algo = s['ror_smoothing_algorithm'] ?? 'moving_average';
		if (!isNaN(windowSec) && windowSec >= 5 && windowSec <= 120) {
			_rorWindowMs.set(windowSec * 1000);
		}
		_rorAlgorithm.set(algo);
	} catch {
		// Use defaults on error
	}
}

type HistoryEntry = { timestamp: number; telemetry: Telemetry };

/** Moving average: endpoint difference over window (current behavior). */
function rorMovingAverage(windowData: HistoryEntry[]): number | null {
	if (windowData.length < 2) return null;
	const first = windowData[0];
	const last = windowData[windowData.length - 1];
	const dt = (last.timestamp - first.timestamp) / 1000;
	if (dt <= 0) return null;
	const dTemp = last.telemetry.beanTemp - first.telemetry.beanTemp;
	return Math.round((dTemp / dt) * 60 * 10) / 10;
}

/** Weighted moving average: linearly weighted slope, recent samples weighted higher. */
function rorWeightedMovingAverage(windowData: HistoryEntry[]): number | null {
	if (windowData.length < 2) return null;
	const n = windowData.length;
	const wSum = (n * (n + 1)) / 2;
	const t0 = windowData[0].timestamp / 1000;

	let sumW = 0,
		sumWT = 0,
		sumWY = 0,
		sumWTT = 0,
		sumWTY = 0;
	for (let i = 0; i < n; i++) {
		const w = (i + 1) / wSum;
		const t = windowData[i].timestamp / 1000 - t0;
		const y = windowData[i].telemetry.beanTemp;
		sumW += w;
		sumWT += w * t;
		sumWY += w * y;
		sumWTT += w * t * t;
		sumWTY += w * t * y;
	}

	const denom = sumW * sumWTT - sumWT * sumWT;
	if (Math.abs(denom) < 1e-10) return null;
	const slope = (sumW * sumWTY - sumWT * sumWY) / denom;
	return Math.round(slope * 60 * 10) / 10;
}

/** Savitzky-Golay: 2nd-order polynomial fit, derivative at center point. */
function rorSavitzkyGolay(windowData: HistoryEntry[]): number | null {
	if (windowData.length < 3) return null;
	const n = windowData.length;
	const tMid = windowData[Math.floor(n / 2)].timestamp / 1000;

	let s0 = 0,
		s1 = 0,
		s2 = 0,
		s3 = 0,
		s4 = 0;
	let r0 = 0,
		r1 = 0,
		r2 = 0;

	for (let i = 0; i < n; i++) {
		const t = windowData[i].timestamp / 1000 - tMid;
		const y = windowData[i].telemetry.beanTemp;
		const t2 = t * t;
		s0 += 1;
		s1 += t;
		s2 += t2;
		s3 += t * t2;
		s4 += t2 * t2;
		r0 += y;
		r1 += t * y;
		r2 += t2 * y;
	}

	// Solve 3x3 normal equations via Cramer's rule
	const det = s0 * (s2 * s4 - s3 * s3) - s1 * (s1 * s4 - s2 * s3) + s2 * (s1 * s3 - s2 * s2);
	if (Math.abs(det) < 1e-10) return null;

	// b coefficient (first derivative at center): replace column 2 with RHS
	const detB = s0 * (r1 * s4 - r2 * s3) - r0 * (s1 * s4 - s2 * s3) + s2 * (s1 * r2 - s2 * r1);
	const b = detB / det;

	// Derivative at t=0 (centered at tMid) is just b; convert to °C/min
	return Math.round(b * 60 * 10) / 10;
}

/** Rate of Rise calculated from telemetry history. */
export const rateOfRise = derived(
	[_history, _rorWindowMs, _rorAlgorithm],
	([$history, $windowMs, $algorithm], set) => {
		const latest = $history[$history.length - 1];
		if (latest && typeof latest.telemetry.rateOfRise === 'number') {
			// The server computes one canonical RoR for every transport.
			set(latest.telemetry.rateOfRise);
			return;
		}
		if ($history.length < 2) {
			set(null);
			return;
		}
		const now = latest.timestamp;
		const windowStart = now - $windowMs;
		const windowData = $history.filter((h) => h.timestamp >= windowStart);
		if (windowData.length < 2) {
			set(null);
			return;
		}

		let result: number | null;
		switch ($algorithm) {
			case 'weighted_moving_average':
				result = rorWeightedMovingAverage(windowData);
				break;
			case 'savitzky_golay':
				result = rorSavitzkyGolay(windowData);
				break;
			default:
				result = rorMovingAverage(windowData);
		}
		set(result);
	},
	null as number | null
);

let ws: WebSocket | null = null;
let reconnectAttempts = 0;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let visibilityHandler: (() => void) | null = null;
let pausedByVisibility = false;
let selectedDevice: string | null = null;
/** Highest sequence number received for `selectedDevice`; resumes gap-free. */
let lastSeq = 0;
/** The server process `lastSeq` belongs to (hello `boot_id`); seqs restart with it. */
let serverBootId: string | null = null;

function sendSubscribe() {
	if (ws?.readyState !== WebSocket.OPEN) return;
	const msg: Record<string, unknown> = { type: 'subscribe' };
	if (selectedDevice) {
		msg.device_id = selectedDevice;
		msg.since_seq = lastSeq;
	}
	ws.send(JSON.stringify(msg));
}

function pushEntries(entries: HistoryEntry[]) {
	if (entries.length === 0) return;
	_history.update((h) => {
		const next = h.length === 0 ? entries : [...h, ...entries];
		return next.length > MAX_HISTORY ? next.slice(next.length - MAX_HISTORY) : next;
	});
	const last = entries[entries.length - 1];
	_telemetry.set(last.telemetry);
}

function handleSamplesFrame(frame: SamplesFrame) {
	if (selectedDevice && frame.device_id !== selectedDevice) return;
	if (frame.catchup && frame.gap) _historyGap.set(true);
	const entries: HistoryEntry[] = [];
	for (const s of frame.samples) {
		// Defensive: a decoder may still hand back BigInt for 64-bit wire integers.
		s.q = Number(s.q);
		s.t = Number(s.t);
		if (s.q > lastSeq) lastSeq = s.q;
		if (!hasTemperature(s)) continue;
		entries.push({ timestamp: s.t, telemetry: sampleToTelemetry(s) });
	}
	if (entries.length === 0) return;
	_deviceId.set(frame.device_id);
	pushEntries(entries);
}

function handleTextFrame(raw: string) {
	const msg = JSON.parse(raw);
	if (msg.type === 'hello') {
		const boot = typeof msg.boot_id === 'string' ? msg.boot_id : null;
		if (boot && serverBootId && boot !== serverBootId && lastSeq > 0) {
			// The server restarted: our seq means nothing to it. Start over.
			lastSeq = 0;
			sendSubscribe();
		}
		if (boot) serverBootId = boot;
		return;
	}
	if (msg.type === 'subscribed') {
		// Sequence numbers live in the server's in-memory ring and restart at 1
		// when the server restarts (every deploy). Resuming from a seq the server
		// has not reached makes it skip every live sample up to that seq: the
		// chart freezes while the socket shows connected. Start over from 0.
		if (
			selectedDevice &&
			msg.device_id === selectedDevice &&
			Number(msg.latest_seq) < Number(msg.from_seq) &&
			lastSeq > Number(msg.latest_seq)
		) {
			lastSeq = 0;
			sendSubscribe();
		}
		return;
	}
	if (msg.type === 'pong' || msg.type === 'error') return;
	if (msg.type === 'session_events') {
		_sessionEventsChanged.set({ session_id: String(msg.session_id), at: Date.now() });
		return;
	}
	if (msg.telemetry) {
		// Protocol 1 compatibility (tests, old servers).
		const tmsg = msg as TelemetryMessage;
		if (selectedDevice && tmsg.device_id !== selectedDevice) return;
		_deviceId.set(tmsg.device_id);
		pushEntries([{ timestamp: Date.now(), telemetry: tmsg.telemetry }]);
	} else if (msg.replay || msg.follow) {
		// `replay` since Phase 2; `follow` from older servers.
		if (selectedDevice && msg.device_id !== selectedDevice) return;
		_replayState.set((msg.replay ?? msg.follow) as ReplayState);
	} else if (msg.alarm) {
		if (selectedDevice && msg.device_id !== selectedDevice) return;
		_alarmEvent.set({ ...(msg.alarm as AlarmEvent), received_at: Date.now() });
	}
}

function connect() {
	if (ws?.readyState === WebSocket.OPEN || ws?.readyState === WebSocket.CONNECTING) return;

	_connectionStatus.set('reconnecting');
	const sock = new WebSocket(WS_URL);
	sock.binaryType = 'arraybuffer';
	ws = sock;
	// Every handler ignores a socket that has been replaced: one closed on
	// `visibilitychange` can report close/error after its successor opened.

	sock.onopen = () => {
		if (ws !== sock) return;
		reconnectAttempts = 0;
		_connectionStatus.set('connected');
		sendSubscribe();
	};

	sock.onmessage = (event) => {
		if (ws !== sock) return;
		try {
			if (typeof event.data === 'string') {
				handleTextFrame(event.data);
			} else {
				// Avoid `instanceof` (fails across realms, e.g. jsdom): build a view over whatever buffer we got.
				const data = event.data as ArrayBuffer | ArrayBufferView;
				const bytes = ArrayBuffer.isView(data)
					? new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
					: new Uint8Array(data);
				const frame = cbor.decode(bytes) as SamplesFrame;
				if (frame?.type === 'samples') handleSamplesFrame(frame);
			}
		} catch {
			// Ignore unparseable messages
		}
	};

	sock.onclose = () => {
		if (ws !== sock) return;
		ws = null;
		_connectionStatus.set('disconnected');
		if (!pausedByVisibility) {
			scheduleReconnect();
		}
	};

	sock.onerror = () => {
		sock.close();
	};
}

function scheduleReconnect() {
	if (reconnectTimer) return;
	const delay = Math.min(1000 * 2 ** reconnectAttempts, MAX_BACKOFF_MS);
	reconnectAttempts++;
	reconnectTimer = setTimeout(() => {
		reconnectTimer = null;
		connect();
	}, delay);
}

/** Clear telemetry history (e.g. when starting a new roast). */
export function clearHistory() {
	_history.set([]);
	_historyGap.set(false);
}

/**
 * Seed history with points recorded by the server (after a page reload during
 * a roast). Points older than the first live sample are prepended; live data
 * is kept as is.
 */
export function seedHistory(points: HistoryEntry[]) {
	if (points.length === 0) return;
	_history.update((h) => {
		const firstLive = h[0]?.timestamp ?? Number.POSITIVE_INFINITY;
		const older = points.filter((p) => p.timestamp < firstLive);
		if (older.length === 0) return h;
		const next = [...older, ...h];
		return next.length > MAX_HISTORY ? next.slice(next.length - MAX_HISTORY) : next;
	});
}

/** Set the device to filter telemetry for. Clears history on device change. */
export function setSelectedDevice(deviceId: string | null) {
	if (deviceId === selectedDevice) return;
	selectedDevice = deviceId;
	lastSeq = 0;
	_telemetry.set(null);
	_deviceId.set(null);
	_history.set([]);
	_replayState.set(null);
	_historyGap.set(false);
	sendSubscribe();
}

/** Sample sequence of the newest entry received for the selected device. */
export function lastSampleSeq(): number {
	return lastSeq;
}

/** Start the WebSocket connection. Call once on app init. */
export function initTelemetrySocket() {
	connect();

	if (typeof document !== 'undefined') {
		visibilityHandler = () => {
			if (document.hidden) {
				// Tab hidden: close connection to save bandwidth
				pausedByVisibility = true;
				if (reconnectTimer) {
					clearTimeout(reconnectTimer);
					reconnectTimer = null;
				}
				ws?.close();
				ws = null;
				_connectionStatus.set('disconnected');
			} else {
				// Tab visible: reconnect immediately (skip backoff)
				pausedByVisibility = false;
				reconnectAttempts = 0;
				connect();
			}
		};
		document.addEventListener('visibilitychange', visibilityHandler);
	}
}

/** Disconnect and clean up. */
export function destroyTelemetrySocket() {
	if (visibilityHandler) {
		document.removeEventListener('visibilitychange', visibilityHandler);
		visibilityHandler = null;
	}
	if (reconnectTimer) {
		clearTimeout(reconnectTimer);
		reconnectTimer = null;
	}
	ws?.close();
	ws = null;
	_connectionStatus.set('disconnected');
}
