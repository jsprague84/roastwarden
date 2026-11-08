import { describe, it, expect } from 'vitest';
import type { RoasterSetup } from '#lib/api/roasters.js';
import {
	allows,
	isRaise,
	effectiveControl,
	needsLine,
	ghostOnlyReason,
	raiseConfirm,
	CONTROL_LEVELS,
	CONTROL_LABEL,
	CONTROL_HELP,
	KINDS,
	KIND_LABEL,
	kindDefaults,
	effectiveOutputs,
	shapeOutput,
	sliderStep,
	outputsProblem,
	outputsLine,
	kindChangeConfirm,
	outputDefault,
	OUTPUT_NAMES,
	OUTPUTS_HELP,
	type ControlFeature
} from './setup.js';

const ALL: ControlFeature[] = [
	'sliders',
	'stop',
	'sequence',
	'tokenAccess',
	'setpoint',
	'drive',
	'autotune',
	'fanAssist',
	'gainSchedule'
];

function setup(device_id: string, control: RoasterSetup['control']): RoasterSetup {
	return {
		device_id,
		kind: 'other',
		control,
		outputs: kindDefaults('other').outputs,
		fan_floor_pct: 0,
		channels: [],
		fixed: false,
		updated_at: null
	};
}

describe('allows', () => {
	it('off allows nothing, not even STOP', () => {
		for (const f of ALL) expect(allows('off', f)).toBe(false);
	});

	it('manual allows the heater, fan, STOP and the sequence only', () => {
		const ok = ALL.filter((f) => allows('manual', f));
		expect(ok.sort()).toEqual(['sequence', 'sliders', 'stop', 'tokenAccess']);
	});

	it('full allows everything', () => {
		for (const f of ALL) expect(allows('full', f)).toBe(true);
	});
});

describe('isRaise', () => {
	it('asks when control goes up, not when it goes down or stays', () => {
		expect(isRaise('off', 'manual')).toBe(true);
		expect(isRaise('off', 'full')).toBe(true);
		expect(isRaise('manual', 'full')).toBe(true);
		expect(isRaise('full', 'manual')).toBe(false);
		expect(isRaise('manual', 'off')).toBe(false);
		expect(isRaise('full', 'full')).toBe(false);
	});
});

describe('effectiveControl', () => {
	it('uses the loaded setup of that roaster', () => {
		expect(effectiveControl(setup('esp32_x', 'off'), 'esp32_x')).toBe('off');
		expect(effectiveControl(setup('esp32_x', 'manual'), 'esp32_x')).toBe('manual');
	});

	it('acts as before until the setup has loaded, or when it is another roaster’s', () => {
		expect(effectiveControl(null, 'esp32_x')).toBe('full');
		expect(effectiveControl(setup('other', 'off'), 'esp32_x')).toBe('full');
	});

	it('the virtual roaster is always full', () => {
		expect(effectiveControl(setup('virtual_roaster', 'off'), 'virtual_roaster')).toBe('full');
	});
});

describe('words', () => {
	it('every level and kind has a label and an explanation', () => {
		for (const l of CONTROL_LEVELS) {
			expect(CONTROL_LABEL[l]).toBeTruthy();
			expect(CONTROL_HELP[l].line).toBeTruthy();
		}
		expect(CONTROL_LABEL.off).toBe('Off · logs only');
		for (const k of KINDS) expect(KIND_LABEL[k]).toBeTruthy();
	});

	it('says what a panel needs only when the level falls short', () => {
		expect(needsLine('fanAssist', 'full')).toBeNull();
		expect(needsLine('fanAssist', 'manual')).toBe(
			"Fan assist needs full control; this roaster's control is manual."
		);
		expect(needsLine('sequence', 'off')).toBe(
			"The roast sequence needs manual or full control; this roaster's control is off."
		);
		expect(needsLine('sequence', 'manual')).toBeNull();
	});

	it('Profile drive offers ghost only below full', () => {
		expect(ghostOnlyReason('full')).toBeNull();
		expect(ghostOnlyReason('manual')).toMatch(/full control/);
		expect(ghostOnlyReason('off')).toMatch(/Control is off/);
	});

	it('the confirm names the roaster', () => {
		expect(raiseConfirm('esp32_x')).toBe(
			'Roastwarden will send commands to esp32_x: the roast sequence at CHARGE, alarm actions, Profile drive. Check its own safety cutoffs first.'
		);
	});
});

describe('outputs', () => {
	it('each kind has the server’s defaults', () => {
		expect(kindDefaults('fluid_bed_electric')).toEqual({
			outputs: {
				heater: { label: 'Heater', min: 0, max: 100, step: 0 },
				fan: { label: 'Fan', min: 0, max: 100, step: 0 },
				drum: null,
				damper: null
			},
			fan_floor_pct: 45
		});
		expect(kindDefaults('drum_gas').outputs.heater?.label).toBe('Burner');
		expect(kindDefaults('drum_gas').outputs.fan?.label).toBe('Air');
		expect(kindDefaults('drum_gas').fan_floor_pct).toBe(0);
		expect(kindDefaults('logger').outputs).toEqual({
			heater: null,
			fan: null,
			drum: null,
			damper: null
		});
		// No kind declares drum speed or a damper: the owner does.
		for (const k of KINDS)
			expect([kindDefaults(k).outputs.drum, kindDefaults(k).outputs.damper]).toEqual([null, null]);
		for (const k of KINDS)
			expect(outputsProblem(kindDefaults(k).outputs, kindDefaults(k).fan_floor_pct)).toBeNull();
	});

	it('acts as before until the setup loads, and always for the simulator', () => {
		expect(effectiveOutputs(null, 'esp32_x')).toEqual(kindDefaults('fluid_bed_electric'));
		const logger = {
			...setup('tc4', 'manual'),
			kind: 'logger' as const,
			outputs: { heater: null, fan: null, drum: null, damper: null }
		};
		expect(effectiveOutputs(logger, 'tc4').outputs).toEqual({
			heater: null,
			fan: null,
			drum: null,
			damper: null
		});
		expect(effectiveOutputs(logger, 'other_one')).toEqual(kindDefaults('fluid_bed_electric'));
		expect(
			effectiveOutputs({ ...logger, device_id: 'virtual_roaster' }, 'virtual_roaster')
		).toEqual(kindDefaults('fluid_bed_electric'));
	});

	it('shapes values as the gate does: 0 off, clamped, rounded to the step', () => {
		const burner = { label: 'Burner', min: 20, max: 80, step: 10 };
		expect(shapeOutput(burner, 95)).toBe(80);
		expect(shapeOutput(burner, 5)).toBe(20);
		expect(shapeOutput(burner, 46)).toBe(50);
		expect(shapeOutput(burner, 0)).toBe(0);
		expect(sliderStep(burner)).toBe(10);
		expect(sliderStep({ ...burner, step: 0 })).toBe(1);
	});

	it('refuses what the server would', () => {
		const ok = kindDefaults('fluid_bed_electric').outputs;
		expect(outputsProblem(ok, 45)).toBeNull();
		expect(
			outputsProblem({ ...ok, heater: { label: 'Heater', min: 80, max: 20, step: 0 } }, 0)
		).toMatch(/min < max/);
		expect(outputsProblem({ ...ok, fan: null }, 45)).toMatch(/both a heater and a fan/);
		expect(outputsProblem({ ...ok, heater: { label: ' ', min: 0, max: 100, step: 0 } }, 0)).toMatch(
			/label/
		);
		expect(outputsProblem(ok, 101)).toMatch(/fan floor/);
	});

	it('checks drum and damper as heater and fan, with no floor involved', () => {
		const ok = kindDefaults('fluid_bed_electric').outputs;
		const drum = { label: 'Drum', min: 30, max: 90, step: 5 };
		expect(
			outputsProblem({ ...ok, drum, damper: { label: 'Damper', min: 0, max: 100, step: 0 } }, 45)
		).toBeNull();
		expect(outputsProblem({ ...ok, drum: { ...drum, label: '' } }, 0)).toBe(
			"The drum's label must be 1 to 24 characters."
		);
		expect(
			outputsProblem({ ...ok, damper: { label: 'Damper', min: 0, max: 120, step: 0 } }, 0)
		).toBe("The damper's range must be 0 ≤ min < max ≤ 100.");
		expect(outputsProblem({ ...ok, drum: { ...drum, step: 70 } }, 0)).toMatch(/drum's step/);
		// An older setup with no drum or damper key: not declared.
		expect(outputsProblem({ heater: ok.heater, fan: ok.fan } as typeof ok, 45)).toBeNull();
		expect(OUTPUT_NAMES).toEqual(['heater', 'fan', 'drum', 'damper']);
		expect(outputDefault('fluid_bed_electric', 'drum')).toEqual({
			label: 'Drum',
			min: 0,
			max: 100,
			step: 0
		});
		expect(outputDefault('logger', 'heater')).toEqual({
			label: 'Heater',
			min: 0,
			max: 100,
			step: 0
		});
		expect(outputDefault('drum_gas', 'heater').label).toBe('Burner');
		expect(OUTPUTS_HELP.more).not.toMatch(/come later/);
	});

	it('says what a roaster has in one line', () => {
		expect(outputsLine(kindDefaults('fluid_bed_electric').outputs, 45)).toBe(
			'Heater 0–100 %, Fan 0–100 % · fan floor 45 % while heating'
		);
		expect(outputsLine(kindDefaults('logger').outputs, 0)).toMatch(/No outputs/);
		const drum = {
			...kindDefaults('fluid_bed_electric').outputs,
			drum: { label: 'Drum', min: 30, max: 90, step: 5 }
		};
		expect(outputsLine(drum, 45)).toBe(
			'Heater 0–100 %, Fan 0–100 %, Drum 30–90 % in 5 % steps · fan floor 45 % while heating'
		);
		expect(kindChangeConfirm('drum_gas')).toContain("a gas drum's defaults");
	});
});
