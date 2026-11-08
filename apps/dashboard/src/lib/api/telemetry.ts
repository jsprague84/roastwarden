/**
 * The WebSocket's telemetry: a roaster's sample as the server sends it (°C; the dashboard converts
 * for display), and the connection's state.
 */

/** Telemetry data structure matching ESP32 JSON output. All temperatures are °C;
 * the dashboard converts for display via `$lib/stores/units.svelte`. */
export interface Telemetry {
	beanTemp: number;
	envTemp: number;
	/** Heater command, % (0-100). */
	heater: number;
	/** Fan, % (0-100). */
	fan: number;
	/** Drum speed the roaster reports, % (only a roaster with a drum output). */
	drum?: number;
	/** Damper position the roaster reports, % (only a roaster with a damper output). */
	damper?: number;
	setpoint: number;
	controlMode: number; // 0 = manual, 1 = auto
	heaterEnable: number; // 0 or 1
	/** Canonical RoR in °C/min, computed by the server for every transport. */
	rateOfRise?: number;
	/** The device's own RoR estimate, if it sent one. */
	deviceRateOfRise?: number;
	/** Server RoR of the environment temperature, °C/min (protocol v2). */
	rateOfRiseEt?: number;
	/** Ambient / room temperature when the device reports one, °C. */
	ambientTemp?: number;
	/** Heater % actually switched after the enable flag, fan interlock and RoR limiter (firmware 3.2+). */
	heaterApplied?: number;
	/** 1 while the RoR limiter cuts the heater (firmware 3.2+). */
	rorLimited?: number;
	Kp?: number;
	Ki?: number;
	Kd?: number;
	uptime?: number;
	freeHeap?: number;
	rssi?: number;
	systemStatus?: number;
	timestamp?: number;
}

/** WebSocket message envelope wrapping telemetry with device context. */
export interface TelemetryMessage {
	device_id: string;
	telemetry: Telemetry;
}

/** WebSocket connection state. */
export type ConnectionStatus = 'connected' | 'reconnecting' | 'disconnected';
