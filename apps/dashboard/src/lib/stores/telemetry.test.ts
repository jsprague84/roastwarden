import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { encode } from 'cbor-x';
// The server's golden WebSocket v2 frame (crates/server/src/analysis/ws2.rs checks it writes these bytes).
import goldenHex from '../../../../../crates/server/testdata/ws2-samples-frame.cbor.hex?raw';
import { get } from 'svelte/store';

// --- WebSocket mock ---

let wsInstances: MockWS[] = [];

class MockWS {
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSING = 2;
	static readonly CLOSED = 3;
	readonly CONNECTING = 0;
	readonly OPEN = 1;
	readonly CLOSING = 2;
	readonly CLOSED = 3;

	readyState = MockWS.CONNECTING;
	url: string;
	onopen: ((ev: Event) => void) | null = null;
	onmessage: ((ev: MessageEvent) => void) | null = null;
	onclose: ((ev: CloseEvent) => void) | null = null;
	onerror: ((ev: Event) => void) | null = null;

	constructor(url: string) {
		this.url = url;
		wsInstances.push(this);
	}

	close() {
		this.readyState = MockWS.CLOSED;
		// Real WebSocket fires onclose asynchronously. Don't fire synchronously
		// here to avoid triggering scheduleReconnect() during destroyTelemetrySocket().
		// Tests should use simulateClose() to trigger onclose explicitly.
	}

	sent: string[] = [];
	binaryType = 'blob';

	send(data: string) {
		this.sent.push(data);
	}

	simulateOpen() {
		this.readyState = MockWS.OPEN;
		this.onopen?.(new Event('open'));
	}

	simulateClose() {
		this.readyState = MockWS.CLOSED;
		this.onclose?.(new CloseEvent('close'));
	}

	simulateMessage(data: string | ArrayBuffer) {
		this.onmessage?.(new MessageEvent('message', { data }));
	}
}

vi.stubGlobal('WebSocket', MockWS);

import {
	telemetry,
	deviceId,
	connectionStatus,
	telemetryHistory,
	rateOfRise,
	clearHistory,
	initTelemetrySocket,
	destroyTelemetrySocket,
	setSelectedDevice,
	lastSampleSeq
} from './telemetry';

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
	return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

function makeTelemetryMsg(beanTemp: number, device = 'test-device') {
	return JSON.stringify({
		device_id: device,
		telemetry: {
			beanTemp,
			envTemp: 150,
			heater: 50,
			fan: 78.4,
			setpoint: 200,
			controlMode: 0,
			heaterEnable: 1
		}
	});
}

describe('telemetry store', () => {
	beforeEach(() => {
		wsInstances = [];
		vi.useFakeTimers();
	});

	afterEach(() => {
		destroyTelemetrySocket();
		vi.clearAllTimers();
		clearHistory();
		vi.useRealTimers();
	});

	it('parses a sample WebSocket message correctly', () => {
		initTelemetrySocket();
		wsInstances[0].simulateOpen();
		wsInstances[0].simulateMessage(makeTelemetryMsg(185.5, 'roaster-1'));

		expect(get(telemetry)).toEqual(
			expect.objectContaining({
				beanTemp: 185.5,
				envTemp: 150,
				heater: 50,
				fan: 78.4,
				setpoint: 200,
				controlMode: 0,
				heaterEnable: 1
			})
		);
		expect(get(deviceId)).toBe('roaster-1');
	});

	it('adds received telemetry to history', () => {
		initTelemetrySocket();
		wsInstances[0].simulateOpen();

		wsInstances[0].simulateMessage(makeTelemetryMsg(180));
		wsInstances[0].simulateMessage(makeTelemetryMsg(182));

		const history = get(telemetryHistory);
		expect(history).toHaveLength(2);
		expect(history[0].telemetry.beanTemp).toBe(180);
		expect(history[1].telemetry.beanTemp).toBe(182);
	});

	it('calculates RoR from history correctly', () => {
		initTelemetrySocket();
		wsInstances[0].simulateOpen();

		// First data point at t=0
		vi.setSystemTime(new Date('2026-01-01T00:00:00'));
		wsInstances[0].simulateMessage(makeTelemetryMsg(180));

		// Second data point at t=30s
		vi.setSystemTime(new Date('2026-01-01T00:00:30'));
		wsInstances[0].simulateMessage(makeTelemetryMsg(190));

		// RoR = (190-180) / 30s * 60 = 20 °C/min
		expect(get(rateOfRise)).toBe(20);
	});

	it('returns null RoR with insufficient history', () => {
		initTelemetrySocket();
		wsInstances[0].simulateOpen();

		// Only one data point
		wsInstances[0].simulateMessage(makeTelemetryMsg(180));

		expect(get(rateOfRise)).toBeNull();
	});

	it('handles reconnection state transitions', () => {
		// Initially disconnected
		expect(get(connectionStatus)).toBe('disconnected');

		// Initiating connection sets status to reconnecting
		initTelemetrySocket();
		expect(get(connectionStatus)).toBe('reconnecting');

		// Opening connection sets status to connected
		wsInstances[0].simulateOpen();
		expect(get(connectionStatus)).toBe('connected');

		// Closing connection sets status to disconnected
		wsInstances[0].simulateClose();
		expect(get(connectionStatus)).toBe('disconnected');

		// Reconnect scheduled — advance past first backoff (1000ms)
		vi.advanceTimersByTime(1100);
		expect(wsInstances).toHaveLength(2);
		expect(get(connectionStatus)).toBe('reconnecting');

		// New connection opens
		wsInstances[1].simulateOpen();
		expect(get(connectionStatus)).toBe('connected');
	});

	it('uses exponential backoff for reconnection', () => {
		initTelemetrySocket();
		wsInstances[0].simulateOpen();

		// First disconnect — backoff = 1000ms * 2^0 = 1000ms
		wsInstances[0].simulateClose();
		vi.advanceTimersByTime(900);
		expect(wsInstances).toHaveLength(1);
		vi.advanceTimersByTime(200);
		expect(wsInstances).toHaveLength(2);

		// Second disconnect — backoff = 1000ms * 2^1 = 2000ms
		wsInstances[1].simulateClose();
		vi.advanceTimersByTime(1900);
		expect(wsInstances).toHaveLength(2);
		vi.advanceTimersByTime(200);
		expect(wsInstances).toHaveLength(3);
	});

	it('subscribes on open and decodes CBOR sample batches with server timestamps', () => {
		initTelemetrySocket();
		const sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		expect(sock.sent.length).toBe(1);
		expect(JSON.parse(sock.sent[0]).type).toBe('subscribe');

		const frame = {
			type: 'samples',
			device_id: 'esp32_roaster_01',
			catchup: true,
			gap: false,
			samples: [
				{ q: 1, t: 1_000, bt: 100, et: 120, h: 10, f: 128, sp: 150, m: 0, he: false },
				{ q: 2, t: 2_000, bt: 101, et: 121, rb: 6, h: 10, f: 128, sp: 150, m: 0, he: false }
			]
		};
		sock.simulateMessage(toArrayBuffer(encode(frame)));

		const history = get(telemetryHistory);
		expect(history.length).toBe(2);
		expect(history[0].timestamp).toBe(1_000);
		expect(history[1].telemetry.rateOfRise).toBe(6);
		expect(get(telemetry)?.beanTemp).toBe(101);
		expect(get(deviceId)).toBe('esp32_roaster_01');
		expect(lastSampleSeq()).toBe(2);
		destroyTelemetrySocket();
	});

	it("decodes the server's golden CBOR frame (ciborium: half floats, u64, extras)", () => {
		const bytes = new Uint8Array(
			goldenHex
				.trim()
				.match(/../g)!
				.map((b: string) => parseInt(b, 16))
		);
		initTelemetrySocket();
		const sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		sock.simulateMessage(toArrayBuffer(bytes));
		const history = get(telemetryHistory);
		expect(history.length).toBe(1);
		expect(history[0].timestamp).toBe(1_792_580_333_555);
		expect(history[0].telemetry.beanTemp).toBe(201.25);
		expect(history[0].telemetry.envTemp).toBe(230.5);
		expect(history[0].telemetry.rateOfRise).toBe(8.75);
		expect(get(deviceId)).toBe('esp32_roaster_01');
		expect(lastSampleSeq()).toBe(3);
		destroyTelemetrySocket();
	});

	it('decodes 64-bit wire integers (server timestamps) as numbers', () => {
		initTelemetrySocket();
		const sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		// ciborium writes u64 as an 8-byte CBOR integer; cbor-x's encoder does the same for a BigInt.
		const frame = {
			type: 'samples',
			device_id: 'dev-b',
			catchup: false,
			samples: [{ q: 7, t: BigInt(1_792_580_333_555), bt: 120 }]
		};
		sock.simulateMessage(toArrayBuffer(encode(frame)));
		const history = get(telemetryHistory);
		expect(history.length).toBe(1);
		expect(typeof history[0].timestamp).toBe('number');
		expect(history[0].timestamp).toBe(1_792_580_333_555);
		expect(lastSampleSeq()).toBe(7);
		destroyTelemetrySocket();
	});

	it('resubscribes from the last sequence after a reconnect', () => {
		initTelemetrySocket();
		let sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		setSelectedDevice('dev-a');
		sock.simulateMessage(
			toArrayBuffer(
				encode({
					type: 'samples',
					device_id: 'dev-a',
					catchup: false,
					samples: [{ q: 41, t: 5_000, bt: 150 }]
				})
			)
		);
		expect(lastSampleSeq()).toBe(41);

		sock.simulateClose();
		vi.advanceTimersByTime(1_000);
		sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		const sub = JSON.parse(sock.sent[sock.sent.length - 1]);
		expect(sub).toEqual({ type: 'subscribe', device_id: 'dev-a', since_seq: 41 });
		destroyTelemetrySocket();
		setSelectedDevice(null);
	});
});

describe('telemetry store: server restart', () => {
	beforeEach(() => {
		wsInstances = [];
		vi.useFakeTimers();
	});
	afterEach(() => {
		destroyTelemetrySocket();
		setSelectedDevice(null);
		vi.useRealTimers();
	});

	it('starts over from seq 0 when the server restarted and is behind our seq', () => {
		initTelemetrySocket();
		let sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		setSelectedDevice('dev-a');
		sock.simulateMessage(
			toArrayBuffer(
				encode({
					type: 'samples',
					device_id: 'dev-a',
					catchup: false,
					samples: [{ q: 5000, t: 5_000, bt: 150 }]
				})
			)
		);
		sock.simulateClose(); // server restarts
		vi.advanceTimersByTime(1_000);
		sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		expect(JSON.parse(sock.sent[sock.sent.length - 1]).since_seq).toBe(5000);
		sock.simulateMessage(
			JSON.stringify({
				type: 'subscribed',
				device_id: 'dev-a',
				from_seq: 5000,
				latest_seq: 12,
				gap: false
			})
		);
		expect(JSON.parse(sock.sent[sock.sent.length - 1])).toEqual({
			type: 'subscribe',
			device_id: 'dev-a',
			since_seq: 0
		});
		sock.simulateMessage(
			toArrayBuffer(
				encode({
					type: 'samples',
					device_id: 'dev-a',
					catchup: true,
					samples: [{ q: 1, t: 9_000, bt: 160 }]
				})
			)
		);
		expect(lastSampleSeq()).toBe(1);
		expect(get(telemetryHistory).at(-1)?.telemetry.beanTemp).toBe(160);
	});

	it('starts over when the hello carries a new boot id', () => {
		initTelemetrySocket();
		let sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		sock.simulateMessage(
			JSON.stringify({ type: 'hello', protocol: 2, boot_id: 'boot-1', rings: [] })
		);
		setSelectedDevice('dev-b');
		sock.simulateMessage(
			toArrayBuffer(
				encode({
					type: 'samples',
					device_id: 'dev-b',
					catchup: false,
					samples: [{ q: 40, t: 4_000, bt: 150 }]
				})
			)
		);
		sock.simulateClose();
		vi.advanceTimersByTime(1_000);
		sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		// Same server: resume from 40.
		expect(JSON.parse(sock.sent[sock.sent.length - 1]).since_seq).toBe(40);
		sock.simulateMessage(
			JSON.stringify({ type: 'hello', protocol: 2, boot_id: 'boot-1', rings: [] })
		);
		expect(JSON.parse(sock.sent[sock.sent.length - 1]).since_seq).toBe(40);
		sock.simulateClose();
		vi.advanceTimersByTime(1_000);
		sock = wsInstances[wsInstances.length - 1];
		sock.simulateOpen();
		// Restarted server (even one already past seq 40): start over from 0.
		sock.simulateMessage(
			JSON.stringify({ type: 'hello', protocol: 2, boot_id: 'boot-2', rings: [] })
		);
		expect(JSON.parse(sock.sent[sock.sent.length - 1])).toEqual({
			type: 'subscribe',
			device_id: 'dev-b',
			since_seq: 0
		});
		// The ack for the stale subscribe does not trigger a third one.
		const n = sock.sent.length;
		sock.simulateMessage(
			JSON.stringify({
				type: 'subscribed',
				device_id: 'dev-b',
				from_seq: 40,
				latest_seq: 12,
				gap: false
			})
		);
		expect(sock.sent).toHaveLength(n);
	});

	it('ignores close events from a socket that was already replaced', () => {
		initTelemetrySocket();
		const old = wsInstances[0];
		old.simulateOpen();
		Object.defineProperty(document, 'hidden', { configurable: true, get: () => true });
		document.dispatchEvent(new Event('visibilitychange'));
		Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
		document.dispatchEvent(new Event('visibilitychange'));
		const fresh = wsInstances[1];
		fresh.simulateOpen();
		old.simulateClose(); // late close of the socket hidden-tab closed
		expect(get(connectionStatus)).toBe('connected');
		vi.advanceTimersByTime(60_000);
		expect(wsInstances).toHaveLength(2);
	});
});
