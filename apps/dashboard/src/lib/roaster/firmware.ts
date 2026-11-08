/**
 * The ESP32 firmware download (Roasters → ESP32 firmware; crates/server/src/roasters/firmware.rs,
 * docs/esp32-fluid-bed.md): the form's defaults and checks, the same as the server's.
 */

export type FirmwareKind = 'starter' | 'fluid_bed';
type ThermocoupleBoard = 'max31855' | 'max6675';

interface FirmwareKindInfo {
	kind: FirmwareKind;
	name: string;
	summary: string;
	guide: string;
}

export interface FirmwareInfo {
	kinds: FirmwareKindInfo[];
	port: number;
	built_in_broker: boolean;
	/** The ids the server has a password for (MQTT_DEVICES, Roasters → MQTT passwords): the
	 *  download fills theirs in */
	roasters: { id: string; added: boolean }[];
}

export interface FirmwareRequest {
	kind: FirmwareKind;
	roaster_id: string;
	password?: string;
	server_address: string;
	port?: number;
	wifi_name?: string;
	wifi_password?: string;
	board: ThermocoupleBoard;
	et_probe: boolean;
}

export interface FirmwareForm {
	kind: FirmwareKind;
	roasterId: string;
	password: string;
	address: string;
	port: number | null;
	wifiName: string;
	wifiPassword: string;
	board: ThermocoupleBoard;
	etProbe: boolean;
}

const LOCAL = new Set(['localhost', '127.0.0.1', '::1', '[::1]']);

/** The address the dashboard was opened at, when an ESP32 on the network can reach it too
 *  (an address or a name, not localhost). */
export function defaultAddress(hostname: string): string {
	return LOCAL.has(hostname) ? '' : hostname;
}

export function idProblem(id: string): string | null {
	const v = id.trim();
	if (!v) return 'Give the roaster an id';
	if (v.length > 48 || !/^[A-Za-z0-9_-]+$/.test(v)) return 'Letters, digits, _ and - only';
	return null;
}

export function addressProblem(address: string): string | null {
	const v = address.trim();
	if (!v) return "The server's address on your network, e.g. 192.168.1.50";
	if (LOCAL.has(v)) return "That is the ESP32 itself: give the server's address on your network";
	if (v.length > 253 || !/^[A-Za-z0-9.-]+$/.test(v))
		return 'Only the address or name, without http:// or a port';
	return null;
}

export function wifiProblem(name: string, password: string): string | null {
	if (new TextEncoder().encode(name).length > 32) return 'A Wi-Fi name has at most 32 bytes';
	if (name && password && password.length < 8) return 'A Wi-Fi password has 8 characters or more';
	if (password.length > 63) return 'A Wi-Fi password has at most 63 characters';
	return null;
}

/** The first problem with the form, or null when it can be downloaded. */
export function formProblem(f: FirmwareForm): string | null {
	return (
		idProblem(f.roasterId) ??
		addressProblem(f.address) ??
		wifiProblem(f.wifiName.trim(), f.wifiPassword)
	);
}

export function firmwareRequest(f: FirmwareForm): FirmwareRequest {
	const req: FirmwareRequest = {
		kind: f.kind,
		roaster_id: f.roasterId.trim(),
		server_address: f.address.trim(),
		board: f.board,
		et_probe: f.etProbe
	};
	if (f.password) req.password = f.password;
	if (f.port != null) req.port = f.port;
	if (f.wifiName.trim()) {
		req.wifi_name = f.wifiName.trim();
		req.wifi_password = f.wifiPassword;
	}
	return req;
}

/** What is left to fill in by hand, from the download's `X-Firmware-Missing`. */
export function missingNote(missing: string[], id: string): string {
	const parts: string[] = [];
	if (missing.includes('wifi')) parts.push('WIFI_NAME and WIFI_PASSWORD');
	if (missing.includes('password')) parts.push(`ROASTER_PASSWORD (${id}'s MQTT password)`);
	return parts.length
		? `Still to fill in at the top of the sketch: ${parts.join(', ')}.`
		: 'Every connection setting is filled in.';
}
