import { describe, expect, it } from 'vitest';
import {
	blankRegister,
	draftOf,
	normalRole,
	REGISTER_ROLES,
	roleProblem,
	rolesFor,
	slaveOf,
	writableRow
} from './registerRoles.js';
import { MODBUS_PRESETS } from '#lib/constants/modbus-presets.js';

const input = (name: string, role: string | null = null) => ({
	name,
	register_type: 'input',
	writable: false,
	role
});
const holding = (name: string, role: string | null = null, writable = true) => ({
	name,
	register_type: 'holding',
	writable,
	role
});

describe('register roles', () => {
	it('a role is on one readable and one writable register at most', () => {
		expect(roleProblem([input('bt', 'bt'), input('et', 'et')])).toBeNull();
		expect(roleProblem([input('heater', 'heater'), holding('heater_cmd', 'heater')])).toBeNull();
		expect(roleProblem([input('a', 'bt'), input('b', 'bt')])).toMatch(
			/BT is on two readable registers \("a" and "b"\)/
		);
		expect(roleProblem([holding('a', 'fan'), holding('b', 'fan')])).toMatch(/two writable/);
		// A holding register not marked writable is a readable one.
		expect(roleProblem([input('a', 'fan'), holding('b', 'fan', false)])).toMatch(/two readable/);
	});

	it('no role is no check', () => {
		expect(roleProblem([input('a'), input('b', ''), input('c', null)])).toBeNull();
		expect(normalRole('  ')).toBeNull();
		expect(normalRole('bt')).toBe('bt');
	});

	it('emergency stop only on a writable holding register or coil', () => {
		expect(roleProblem([holding('stop', 'emergency_stop')])).toBeNull();
		expect(
			roleProblem([{ name: 'e', register_type: 'coil', writable: true, role: 'emergency_stop' }])
		).toBeNull();
		expect(roleProblem([input('stop', 'emergency_stop')])).toMatch(
			/emergency stop needs a writable/
		);
		expect(roleProblem([holding('stop', 'emergency_stop', false)])).toMatch(/emergency stop/);
	});

	it('names an unnamed row by its number, refuses an unknown role', () => {
		expect(roleProblem([input('', 'bt'), input('', 'bt')])).toMatch(/row 1 and row 2/);
		expect(roleProblem([input('x', 'speed')])).toMatch(/unknown role/);
	});

	it('offers emergency stop only on a writable row', () => {
		expect(rolesFor(holding('s'))).toEqual(REGISTER_ROLES);
		expect(rolesFor(input('s'))).not.toContain('emergency_stop');
		expect(writableRow({ name: 'x', register_type: 'input', writable: true })).toBe(false);
	});

	it('every preset passes the check', () => {
		for (const p of MODBUS_PRESETS) expect(roleProblem(p.registers)).toBeNull();
		const std = MODBUS_PRESETS.find((p) => p.id === 'roastwarden-standard')!;
		const role = (n: string) => std.registers.find((r) => r.name === n)?.role;
		expect(role('bean_temp')).toBe('bt');
		expect(role('heater_cmd')).toBe('heater');
		expect(role('emergency_stop')).toBe('emergency_stop');
	});

	it('a saved register becomes an editable row, the server’s fields dropped', () => {
		const saved = {
			id: 'r1',
			device_id: 'd1',
			register_type: 'holding',
			address: 16,
			name: 'heater_cmd',
			data_type: 'uint16',
			byte_order: 'AB',
			scale_factor: 1,
			offset: 0,
			writable: true,
			role: ''
		};
		const row = draftOf(saved);
		expect(row).not.toHaveProperty('id');
		expect(row).not.toHaveProperty('device_id');
		expect(row).toMatchObject({
			address: 16,
			name: 'heater_cmd',
			writable: true,
			role: null,
			unit: '',
			description: ''
		});
		expect(blankRegister()).toMatchObject({
			register_type: 'input',
			address: 0,
			name: '',
			data_type: 'uint16',
			writable: false,
			role: null
		});
	});
});

describe('a register on its own slave', () => {
	it("keeps 1–247 and reads anything else as the connection's", () => {
		expect(slaveOf(2)).toBe(2);
		expect(slaveOf('12')).toBe(12);
		expect(slaveOf('')).toBeNull();
		expect(slaveOf(0)).toBeNull();
		expect(slaveOf(248)).toBeNull();
		expect(slaveOf(null)).toBeNull();
		expect(
			draftOf({ register_type: 'input', address: 0, name: 'ET', data_type: 'uint16', unit_id: 2 })
				.unit_id
		).toBe(2);
		expect(blankRegister().unit_id).toBeNull();
	});

	it('refuses a slave outside 1–247 as the server does', () => {
		const row = { register_type: 'input', name: 'ET', role: 'et' };
		expect(roleProblem([{ ...row, unit_id: 2 }])).toBeNull();
		expect(roleProblem([{ ...row, unit_id: 300 }])).toMatch(/slave is 1 to 247/);
	});
});
