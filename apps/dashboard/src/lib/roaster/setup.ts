/**
 * A roaster's setup in words and rules (docs/roaster-setup.md): what each control level
 * lets Roastwarden send, and what the dashboard shows for it. The server enforces the same
 * rules; these only keep the page from offering what would be refused.
 */
import type { Help } from '#lib/constants/help.js';
import type {
	ControlLevel,
	RoasterKind,
	RoasterOutput,
	RoasterOutputs,
	RoasterSetup
} from '#lib/api/roasters.js';
import { isSimulatedFluidBed } from '#lib/roast/live.js';

/** What the dashboard can do to a roaster, each tied to a control level. */
export type ControlFeature =
	/** The heater and fan sliders and the heater switch */
	| 'sliders'
	/** Emergency stop */
	| 'stop'
	/** The roast sequence's fan and heater steps at CHARGE, DROP and cancel */
	| 'sequence'
	/** Setpoint, mode (Manual/Auto) and PID gains */
	| 'setpoint'
	/** Profile drive in drive mode (ghost runs need no control) */
	| 'drive'
	| 'autotune'
	| 'fanAssist'
	| 'gainSchedule'
	/** Whether API tokens may control it (meaningless with nothing to control) */
	| 'tokenAccess';

const NEEDS: Record<ControlFeature, Exclude<ControlLevel, 'off'>> = {
	sliders: 'manual',
	stop: 'manual',
	sequence: 'manual',
	tokenAccess: 'manual',
	setpoint: 'full',
	drive: 'full',
	autotune: 'full',
	fanAssist: 'full',
	gainSchedule: 'full'
};

const RANK: Record<ControlLevel, number> = { off: 0, manual: 1, full: 2 };

export const CONTROL_LEVELS: ControlLevel[] = ['off', 'manual', 'full'];

/** Whether a control level lets the feature act on the roaster. */
export function allows(level: ControlLevel, feature: ControlFeature): boolean {
	return RANK[level] >= RANK[NEEDS[feature]];
}

/** Going from `from` to `to` lets Roastwarden send more: ask before it. */
export function isRaise(from: ControlLevel, to: ControlLevel): boolean {
	return RANK[to] > RANK[from];
}

/** The level the dashboard acts on. Until the setup has loaded it assumes full, as before
 * setups existed (the server refuses what the setup does not allow, so nothing is sent that
 * should not be); the virtual roaster is always full. */
export function effectiveControl(
	setup: RoasterSetup | null,
	deviceId: string | null
): ControlLevel {
	if (isSimulatedFluidBed(deviceId)) return 'full';
	if (!setup || setup.device_id !== deviceId) return 'full';
	return setup.control;
}

export const CONTROL_LABEL: Record<ControlLevel, string> = {
	off: 'Off · logs only',
	manual: 'Manual',
	full: 'Full'
};

export const CONTROL_HELP: Record<ControlLevel, Help> = {
	off: {
		line: 'Roastwarden records this roaster and sends it nothing, not even STOP.',
		more: 'Recording, the scope, RoR, events, alarms that notify, profiles as a background, history, guidance, reports and labels all work. The roast sequence, alarm actions, Profile drive and the controls do nothing: use the roaster’s own controls. New roasters start here.'
	},
	manual: {
		line: 'Roastwarden may set the heater %, the fan % and the heater switch, and send STOP.',
		more: 'The heater and fan sliders, STOP, alarm actions of those kinds and the roast sequence’s fan and heater steps work. Setpoint, mode, PID gains, Profile drive, autotune, fan assist and the gain schedule need full control.'
	},
	full: {
		line: 'Roastwarden may send everything: setpoint, mode, PID gains, Profile drive, autotune.',
		more: 'Everything works: the server PID, Profile drive, autotune, the gain schedule and fan assist, as well as the sliders, STOP and the roast sequence.'
	}
};

export const KIND_LABEL: Record<RoasterKind, string> = {
	fluid_bed_electric: 'Fluid bed, electric',
	drum_electric: 'Drum, electric',
	drum_gas: 'Drum, gas',
	logger: 'Logger only (reads temperatures)',
	other: 'Other'
};

export const KINDS: RoasterKind[] = [
	'fluid_bed_electric',
	'drum_electric',
	'drum_gas',
	'logger',
	'other'
];

export const KIND_HELP: Help = {
	line: 'What the roaster is. Choosing one sets its outputs, fan floor, roast sequence and fan assist.',
	more: 'A fluid bed: heater and fan 0–100 %, a 45 % fan floor while heating, and the roast sequence at CHARGE and DROP. A drum (electric or gas): heater or burner and fan or air, no floor, and a sequence that only records (a drum is preheated and keeps running). A logger: no outputs. Edit the outputs after choosing; the control level still decides whether anything is sent.'
};

// ---------------------------------------------------------------- outputs --

export type OutputName = 'heater' | 'fan' | 'drum' | 'damper';

/** Every output, in the order the setup editor and the outputs line list them. */
export const OUTPUT_NAMES: OutputName[] = ['heater', 'fan', 'drum', 'damper'];

const pct = (label: string): RoasterOutput => ({ label, min: 0, max: 100, step: 0 });

/** The fluid bed's fan floor: a margin above its firmware's 40 % heater interlock. */
const FLUID_BED_FAN_FLOOR = 45;

/** A kind's outputs and fan floor, as the server writes them (docs/roaster-setup.md). */
export function kindDefaults(kind: RoasterKind): {
	outputs: RoasterOutputs;
	fan_floor_pct: number;
} {
	switch (kind) {
		case 'fluid_bed_electric':
			return {
				outputs: { heater: pct('Heater'), fan: pct('Fan'), drum: null, damper: null },
				fan_floor_pct: FLUID_BED_FAN_FLOOR
			};
		case 'drum_gas':
			return {
				outputs: { heater: pct('Burner'), fan: pct('Air'), drum: null, damper: null },
				fan_floor_pct: 0
			};
		case 'logger':
			return { outputs: { heater: null, fan: null, drum: null, damper: null }, fan_floor_pct: 0 };
		default:
			return {
				outputs: { heater: pct('Heater'), fan: pct('Fan'), drum: null, damper: null },
				fan_floor_pct: 0
			};
	}
}

/** The outputs the dashboard acts on. Until the setup has loaded, both at 0–100 % with the
 * fluid bed's floor, as before outputs existed (the server shapes what it sends anyway). */
export function effectiveOutputs(
	setup: RoasterSetup | null,
	deviceId: string | null
): { outputs: RoasterOutputs; fan_floor_pct: number } {
	if (!setup || setup.device_id !== deviceId || isSimulatedFluidBed(deviceId))
		return kindDefaults('fluid_bed_electric');
	return { outputs: setup.outputs, fan_floor_pct: setup.fan_floor_pct };
}

/** The value the roaster gets, as the server's gate shapes it: 0 is off; else clamped and
 * rounded to the step. */
export function shapeOutput(o: RoasterOutput, v: number): number {
	if (!(v > 0)) return 0;
	let x = Math.min(o.max, Math.max(o.min, v));
	if (o.step > 0)
		x = Math.min(o.max, Math.max(o.min, o.min + Math.round((x - o.min) / o.step) * o.step));
	return x;
}

/** The slider's step: the output's, else 1 %. */
export function sliderStep(o: RoasterOutput): number {
	return o.step > 0 ? o.step : 1;
}

/** What an output is switched on as in the setup editor: the kind's default, or for drum and
 * damper (no kind declares them) the label and 0–100 %. */
export function outputDefault(kind: RoasterKind, name: OutputName): RoasterOutput {
	return (
		kindDefaults(kind).outputs[name] ??
		kindDefaults('other').outputs[name] ??
		pct(OUTPUT_LABEL[name])
	);
}

/** An output's name in the setup editor. */
export const OUTPUT_LABEL: Record<OutputName, string> = {
	heater: 'Heater',
	fan: 'Fan',
	drum: 'Drum',
	damper: 'Damper'
};

/** Why outputs and a floor would be refused, or null (the server's rule). Drum and damper
 * may be missing from an older setup: treated as not declared. */
export function outputsProblem(outputs: RoasterOutputs, floor: number): string | null {
	for (const name of OUTPUT_NAMES) {
		const o = outputs[name] ?? null;
		if (!o) continue;
		const n = o.label.trim().length;
		if (n < 1 || n > 24) return `The ${name}'s label must be 1 to 24 characters.`;
		if (![o.min, o.max, o.step].every(Number.isFinite))
			return `The ${name}'s range must be numbers.`;
		if (!(o.min >= 0 && o.min < o.max && o.max <= 100))
			return `The ${name}'s range must be 0 ≤ min < max ≤ 100.`;
		if (!(o.step >= 0 && o.step <= o.max - o.min))
			return `The ${name}'s step must be 0 to (max − min).`;
	}
	if (!(Number.isFinite(floor) && floor >= 0 && floor <= 100))
		return 'The fan floor must be 0 to 100 %.';
	if (floor > 0) {
		if (!outputs.heater || !outputs.fan) return 'A fan floor needs both a heater and a fan.';
		if (floor > outputs.fan.max) return "The fan floor must be at or below the fan's max.";
	}
	return null;
}

/** One line for a roaster's outputs: "Heater 0–100 %, Fan 0–100 % · fan floor 45 %". */
export function outputsLine(outputs: RoasterOutputs, floor: number): string {
	const one = (o: RoasterOutput) =>
		`${o.label} ${o.min}–${o.max} %${o.step > 0 ? ` in ${o.step} % steps` : ''}`;
	const parts = OUTPUT_NAMES.map((n) => outputs[n] ?? null)
		.filter((o): o is RoasterOutput => o !== null)
		.map(one);
	if (parts.length === 0) return 'No outputs: Roastwarden only records it.';
	return parts.join(', ') + (floor > 0 ? ` · fan floor ${floor} % while heating` : '');
}

/** Asked before a kind change, which rewrites the outputs, the sequence and fan assist. */
export function kindChangeConfirm(kind: RoasterKind): string {
	return `Changing the kind sets the outputs, the fan floor, the roast sequence and fan assist to ${KIND_NOUN[kind]} defaults.`;
}

const KIND_NOUN: Record<RoasterKind, string> = {
	fluid_bed_electric: "an electric fluid bed's",
	drum_electric: "an electric drum's",
	drum_gas: "a gas drum's",
	logger: "a logger's",
	other: 'the general'
};

export const OUTPUTS_HELP: Help = {
	line: 'What Roastwarden may set on this roaster, and the range it keeps to.',
	more: 'An output that is off here is never sent and its control is hidden (a logger has none; a roaster may have a heater and no fan). Values outside the range are sent as the nearest end; with a step they round to it. 0 always means off. Drum speed and the damper are declared here when the roaster has them; only then does the Roast page’s Control tab show their sliders. Profile drive, alarms and the roast sequence do not set them yet.'
};

export const FAN_FLOOR_HELP: Help = {
	line: 'While the heater is on, the fan never goes below this. 0: none.',
	more: 'A fluid bed needs air through the beans while it heats: its firmware cuts the heater below 40 % fan, and Roastwarden keeps 45 %. The roast sequence, fan assist, Profile drive and the autotune keep to it, and a fan set lower by hand while the heater is on is raised to it.'
};

/** The Roast page's line where the controls would be, and STOP's tooltip, with control off. */
export const CONTROL_OFF_MESSAGE =
	"Control is off for this roaster: Roastwarden only records it. Use the roaster's own controls.";

/** Asked once before raising a roaster's control level. */
export function raiseConfirm(deviceId: string): string {
	return `Roastwarden will send commands to ${deviceId}: the roast sequence at CHARGE, alarm actions, Profile drive. Check its own safety cutoffs first.`;
}

const FEATURE_NAME: Record<ControlFeature, string> = {
	sliders: 'The heater and fan controls',
	stop: 'Emergency stop',
	sequence: 'The roast sequence',
	setpoint: 'Setpoint, mode and PID gains',
	drive: 'Driving the roaster from a profile',
	autotune: 'Autotune',
	fanAssist: 'Fan assist',
	gainSchedule: 'The gain schedule',
	tokenAccess: 'Token access'
};

/** One line where a feature's panel would be, or null when the level allows it. */
export function needsLine(feature: ControlFeature, level: ControlLevel): string | null {
	if (allows(level, feature)) return null;
	const need = NEEDS[feature] === 'full' ? 'full control' : 'manual or full control';
	return `${FEATURE_NAME[feature]} needs ${need}; this roaster's control is ${level === 'off' ? 'off' : level}.`;
}

/** Why Profile drive offers only a ghost run, or null when it may drive. */
export function ghostOnlyReason(level: ControlLevel): string | null {
	if (allows(level, 'drive')) return null;
	return level === 'off'
		? 'Control is off for this roaster: only a ghost run (the overlay, nothing sent).'
		: 'Driving needs full control (this roaster is manual): only a ghost run (the overlay, nothing sent).';
}
