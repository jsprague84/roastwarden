type ModbusRegisterEntry = {
	address: number;
	name: string;
	register_type: string;
	data_type: string;
	byte_order: string;
	scale_factor: number;
	offset: number;
	unit: string;
	writable: boolean;
	description: string;
	/** What the register is (lib/roaster/registerRoles.ts); null: by its name, as before roles. */
	role: string | null;
};

export type ModbusPreset = {
	id: string;
	name: string;
	description: string;
	registers: ModbusRegisterEntry[];
};

/**
 * Roastwarden Standard register layout matching the backend modbus.rs implementation.
 * Input registers for sensor telemetry, holding registers for control values.
 */
const ROASTWARDEN_STANDARD: ModbusRegisterEntry[] = [
	// Input registers (read-only, telemetry)
	{
		address: 0x0000,
		name: 'bean_temp',
		register_type: 'input',
		data_type: 'float32',
		byte_order: 'ABCD',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '\u00B0C',
		writable: false,
		description: 'Bean temperature (float32 across 0x0000-0x0001)',
		role: 'bt'
	},
	{
		address: 0x0002,
		name: 'env_temp',
		register_type: 'input',
		data_type: 'float32',
		byte_order: 'ABCD',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '\u00B0C',
		writable: false,
		description: 'Environment temperature (float32 across 0x0002-0x0003)',
		role: 'et'
	},
	{
		address: 0x0004,
		name: 'rate_of_rise',
		register_type: 'input',
		data_type: 'float32',
		byte_order: 'ABCD',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '\u00B0C/min',
		writable: false,
		description: 'Rate of rise (float32 across 0x0004-0x0005)',
		role: 'ror'
	},
	{
		address: 0x0006,
		name: 'heater',
		register_type: 'input',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '%',
		writable: false,
		description: 'Heater command, % (0-100)',
		role: 'heater'
	},
	{
		address: 0x0007,
		name: 'fan',
		register_type: 'input',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '%',
		writable: false,
		description: 'Fan, % (0-100)',
		role: 'fan'
	},
	// Holding registers (read-write, control)
	{
		address: 0x0000,
		name: 'setpoint',
		register_type: 'holding',
		data_type: 'float32',
		byte_order: 'ABCD',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '\u00B0C',
		writable: true,
		description: 'Target bean temperature (float32 across 0x0000-0x0001)',
		role: 'setpoint'
	},
	{
		address: 0x0002,
		name: 'fan_cmd',
		register_type: 'holding',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '%',
		writable: true,
		description: 'Fan command, % (0-100)',
		role: 'fan'
	},
	{
		address: 0x0003,
		name: 'heater_cmd',
		register_type: 'holding',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '%',
		writable: true,
		description: 'Heater command, % (0-100)',
		role: 'heater'
	},
	{
		address: 0x0004,
		name: 'control_mode',
		register_type: 'holding',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '',
		writable: true,
		description: 'Control mode: 0 = manual, 1 = auto',
		role: 'mode'
	},
	{
		address: 0x0005,
		name: 'heater_enable',
		register_type: 'holding',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '',
		writable: true,
		description: 'Heater enable: 0 = off, 1 = on',
		role: 'heater_enable'
	},
	{
		address: 0x000c,
		name: 'emergency_stop',
		register_type: 'holding',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '',
		writable: true,
		description: 'Emergency stop: write 1 to trigger',
		role: 'emergency_stop'
	}
];

/**
 * Artisan-compatible register layout following the conventions used by Artisan roasting software.
 * Uses the standard PID controller register addresses for temperature, setpoint, heater, and fan.
 * Based on the Artisan Modbus device setup documentation.
 */
const ARTISAN_COMPATIBLE: ModbusRegisterEntry[] = [
	// Input registers — sensor readings
	{
		address: 0x0000,
		name: 'BT',
		register_type: 'input',
		data_type: 'int16',
		byte_order: 'AB',
		scale_factor: 0.1,
		offset: 0.0,
		unit: '\u00B0C',
		writable: false,
		description: 'Bean Temperature (BT) x10',
		role: 'bt'
	},
	{
		address: 0x0001,
		name: 'ET',
		register_type: 'input',
		data_type: 'int16',
		byte_order: 'AB',
		scale_factor: 0.1,
		offset: 0.0,
		unit: '\u00B0C',
		writable: false,
		description: 'Environment Temperature (ET) x10',
		role: 'et'
	},
	{
		address: 0x0002,
		name: 'BT_RoR',
		register_type: 'input',
		data_type: 'int16',
		byte_order: 'AB',
		scale_factor: 0.1,
		offset: 0.0,
		unit: '\u00B0C/min',
		writable: false,
		description: 'BT Rate of Rise x10',
		role: 'ror'
	},
	{
		address: 0x0003,
		name: 'heater_duty',
		register_type: 'input',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '%',
		writable: false,
		description: 'Heater duty cycle (0-100)',
		role: 'heater'
	},
	{
		address: 0x0004,
		name: 'fan_duty',
		register_type: 'input',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '%',
		writable: false,
		description: 'Fan duty cycle (0-100)',
		role: 'fan'
	},
	// Holding registers — control setpoints (Artisan s/w write targets)
	{
		address: 0x0000,
		name: 'SV',
		register_type: 'holding',
		data_type: 'int16',
		byte_order: 'AB',
		scale_factor: 0.1,
		offset: 0.0,
		unit: '\u00B0C',
		writable: true,
		description: 'Set Value / target temperature x10',
		role: 'setpoint'
	},
	{
		address: 0x0001,
		name: 'heater_cmd',
		register_type: 'holding',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '%',
		writable: true,
		description: 'Heater command (0-100)',
		role: 'heater'
	},
	{
		address: 0x0002,
		name: 'fan_cmd',
		register_type: 'holding',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '%',
		writable: true,
		description: 'Fan command (0-100)',
		role: 'fan'
	},
	{
		address: 0x0003,
		name: 'pid_mode',
		register_type: 'holding',
		data_type: 'uint16',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '',
		writable: true,
		description: 'PID mode: 0 = manual, 1 = auto',
		role: 'mode'
	},
	// Coil registers — discrete controls
	{
		address: 0x0000,
		name: 'heater_on',
		register_type: 'coil',
		data_type: 'bool',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '',
		writable: true,
		description: 'Heater enable (on/off)',
		role: 'heater_enable'
	},
	{
		address: 0x0001,
		name: 'fan_on',
		register_type: 'coil',
		data_type: 'bool',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '',
		writable: true,
		description: 'Fan enable (on/off)',
		role: null
	},
	{
		address: 0x0002,
		name: 'e_stop',
		register_type: 'coil',
		data_type: 'bool',
		byte_order: 'AB',
		scale_factor: 1.0,
		offset: 0.0,
		unit: '',
		writable: true,
		description: 'Emergency stop',
		role: 'emergency_stop'
	}
];

export const MODBUS_PRESETS: ModbusPreset[] = [
	{
		id: 'roastwarden-standard',
		name: 'Roastwarden Standard',
		description:
			'Standard register layout matching the Roastwarden backend Modbus server. Float32 temperatures, uint16 controls.',
		registers: ROASTWARDEN_STANDARD
	},
	{
		id: 'artisan-compatible',
		name: 'Artisan Compatible',
		description:
			'Register layout compatible with Artisan roasting software. Int16 temperatures (x10 scale), PID controller style.',
		registers: ARTISAN_COMPATIBLE
	},
	{
		id: 'custom-empty',
		name: 'Custom (Empty)',
		description: 'Start with a blank register map and define your own registers.',
		registers: []
	}
];
