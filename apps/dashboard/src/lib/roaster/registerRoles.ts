/**
 * A Modbus register's role (docs/ingest-mapping.md, "Modbus TCP and RTU"): what the register
 * is, whatever its name. Read: a register with a role is reported as that field; write: a
 * command goes to the writable register with its role. The server refuses (400) a map where a
 * role is on more than one readable or more than one writable register, or `emergency_stop`
 * is on anything but a writable holding register or coil; `roleProblem` mirrors that rule.
 */

export type RegisterRole =
	| 'bt'
	| 'et'
	| 'ambient'
	| 'ror'
	| 'heater'
	| 'heater_applied'
	| 'fan'
	| 'drum'
	| 'damper'
	| 'setpoint'
	| 'mode'
	| 'heater_enable'
	| 'emergency_stop';

export const REGISTER_ROLES: RegisterRole[] = [
	'bt',
	'et',
	'ambient',
	'ror',
	'heater',
	'heater_applied',
	'fan',
	'drum',
	'damper',
	'setpoint',
	'mode',
	'heater_enable',
	'emergency_stop'
];

export const ROLE_LABEL: Record<RegisterRole, string> = {
	bt: 'BT',
	et: 'ET',
	ambient: 'Ambient',
	ror: 'RoR',
	heater: 'Heater',
	heater_applied: 'Heater applied',
	fan: 'Fan',
	drum: 'Drum',
	damper: 'Damper',
	setpoint: 'Setpoint',
	mode: 'Mode',
	heater_enable: 'Heater enable',
	emergency_stop: 'Emergency stop'
};

/** Roles only a command writes: never on a read-only register. */
const WRITE_ONLY: ReadonlySet<string> = new Set(['emergency_stop']);

/** What the role check needs of a register. */
export interface RoleRow {
	name: string;
	register_type: string;
	writable?: boolean;
	role?: string | null;
}

/** A register that commands may write: a holding register or coil marked writable. */
export function writableRow(r: RoleRow): boolean {
	return !!r.writable && (r.register_type === 'holding' || r.register_type === 'coil');
}

/** The roles a register may take: write-only roles only on a writable one. */
export function rolesFor(r: RoleRow): RegisterRole[] {
	return writableRow(r) ? REGISTER_ROLES : REGISTER_ROLES.filter((x) => !WRITE_ONLY.has(x));
}

/** An empty role ("" or null) is none: today's behaviour by name. */
export function normalRole(role: string | null | undefined): string | null {
	const r = role?.trim();
	return r ? r : null;
}

const rowName = (r: RoleRow, i: number) => (r.name.trim() ? `"${r.name.trim()}"` : `row ${i + 1}`);

/** Why the server would refuse the map's roles (or a register's slave), or null. */
export function roleProblem(rows: (RoleRow & { unit_id?: number | null })[]): string | null {
	for (const [i, r] of rows.entries()) {
		const u = r.unit_id;
		if (u !== null && u !== undefined && !(Number.isInteger(u) && u >= 1 && u <= 247))
			return `${rowName(r, i)}: a slave is 1 to 247 (empty: the connection's).`;
	}
	const seen = new Map<string, number>();
	for (const [i, r] of rows.entries()) {
		const role = normalRole(r.role);
		if (!role) continue;
		if (!(REGISTER_ROLES as string[]).includes(role))
			return `${rowName(r, i)} has an unknown role "${role}".`;
		const w = writableRow(r);
		if (WRITE_ONLY.has(role) && !w)
			return `${rowName(r, i)}: emergency stop needs a writable holding register or coil.`;
		const key = `${role}/${w ? 'w' : 'r'}`;
		const first = seen.get(key);
		if (first !== undefined) {
			const label = ROLE_LABEL[role as RegisterRole];
			return `${label} is on two ${w ? 'writable' : 'readable'} registers (${rowName(rows[first], first)} and ${rowName(r, i)}): one each at most.`;
		}
		seen.set(key, i);
	}
	return null;
}

/** The editable part of a register (what `PUT /api/devices/{id}/register-map` takes). */
export interface RegisterDraft extends RoleRow {
	address: number;
	/** uint16, int16, uint32, int32, float32, bcd16, bcd32 or bool */
	data_type: string;
	byte_order?: string;
	scale_factor?: number;
	offset?: number;
	unit?: string;
	description?: string;
	/** Its own Modbus slave, 1–247 (a bus of several controllers); null: the connection's. */
	unit_id?: number | null;
}

/** A slave address as the editor's field gives it: 1–247, anything else (empty) the connection's. */
export function slaveOf(v: unknown): number | null {
	const n = typeof v === 'number' ? v : typeof v === 'string' && v.trim() !== '' ? Number(v) : NaN;
	return Number.isInteger(n) && n >= 1 && n <= 247 ? n : null;
}

/** A saved register or a preset's as the editor's row: the server's id and device dropped. */
export function draftOf(r: RegisterDraft): RegisterDraft {
	return {
		register_type: r.register_type,
		address: r.address,
		name: r.name,
		data_type: r.data_type,
		byte_order: r.byte_order ?? 'AB',
		scale_factor: r.scale_factor ?? 1.0,
		offset: r.offset ?? 0.0,
		unit: r.unit ?? '',
		description: r.description ?? '',
		writable: r.writable ?? false,
		role: normalRole(r.role),
		unit_id: slaveOf(r.unit_id)
	};
}

/** A new row: an input register at 0, by its name until it gets a role. */
export function blankRegister(): RegisterDraft {
	return draftOf({ register_type: 'input', address: 0, name: '', data_type: 'uint16' });
}
