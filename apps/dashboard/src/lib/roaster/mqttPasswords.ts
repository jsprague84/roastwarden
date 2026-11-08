/**
 * Roasters' MQTT passwords (Roasters → MQTT passwords; crates/server/src/roasters/mqtt_passwords.rs,
 * docs/add-roaster.md "MQTT passwords"): who may sign in to the built-in broker, and the
 * checks the server makes, so the form says what is wrong before it is sent.
 */

type PasswordSource = 'env' | 'dashboard';

export interface MqttPassword {
	roaster_id: string;
	source: PasswordSource;
	created_at?: string | null;
	changed_at?: string | null;
	/** Connected to the broker now */
	signed_in: boolean;
	/** Set up under Roasters already */
	added: boolean;
}

export interface MqttPasswordList {
	/** The server runs its own MQTT broker; without it none can be added here */
	built_in_broker: boolean;
	passwords: MqttPassword[];
}

/** Answered once, when a password is added or changed */
export interface MqttPasswordGiven {
	roaster_id: string;
	password: string;
}

const RESERVED = new Set(['roastwarden', 'homeassistant']);

export function roasterIdProblem(id: string): string | null {
	const v = id.trim();
	if (!v) return 'Give the roaster an id';
	if (v.length > 48 || !/^[A-Za-z0-9_-]+$/.test(v))
		return 'Letters, digits, _ and - only, at most 48';
	if (RESERVED.has(v)) return `“${v}” is the broker's own`;
	return null;
}

/** A password of your own (empty: the server makes a random one). */
export function passwordProblem(pw: string): string | null {
	if (!pw) return null;
	if (pw.length < 8 || pw.length > 128) return '8 to 128 characters';
	if (!/^[\x21-\x7e]+$/.test(pw) || /["'`\\]/.test(pw)) return 'No spaces, quotes or backslashes';
	return null;
}

/** Where a row's password lives, in a few words. */
export function sourceNote(p: MqttPassword, date: (iso: string) => string): string {
	if (p.source === 'env') return 'In .env (MQTT_DEVICES)';
	const changed =
		p.changed_at && p.changed_at !== p.created_at ? `, new password ${date(p.changed_at)}` : '';
	return p.created_at ? `Added ${date(p.created_at)}${changed}` : 'Added here';
}

/** The ids already taken, to say so before the server does. */
export function taken(list: MqttPasswordList | null, id: string): boolean {
	const v = id.trim();
	return !!list?.passwords.some((p) => p.roaster_id === v);
}
