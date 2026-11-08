/**
 * A roaster's setup (docs/roaster-setup.md): its kind, control level, outputs, fan floor and
 * declared channels; and the device contract (docs/control.md).
 */

import { request } from '#lib/api/http.js';
import type { DeviceCaps } from '#lib/api/replay.js';
import type { Gains } from '#lib/api/tuning.js';

export type RoasterKind = 'fluid_bed_electric' | 'drum_electric' | 'drum_gas' | 'logger' | 'other';

/** What Roastwarden may send the roaster: nothing, heater/fan/STOP, or everything. */
export type ControlLevel = 'off' | 'manual' | 'full';

/** One output the roaster accepts (heater or burner, fan or air, drum speed, damper), in %. */
export interface RoasterOutput {
	/** What the dashboard calls it: "Heater", "Burner", "Fan", "Air". */
	label: string;
	/** Lowest value when on; 0 always passes as off. */
	min: number;
	max: number;
	/** Values round to a multiple of this from `min`; 0: any value. */
	step: number;
}

/** A null output is not declared: its commands are refused and its controls hidden. */
export interface RoasterOutputs {
	heater: RoasterOutput | null;
	fan: RoasterOutput | null;
	/** Drum speed: no kind declares it; the owner does when the roaster has one. */
	drum: RoasterOutput | null;
	/** The damper (airflow): no kind declares it; the owner does when the roaster has one. */
	damper: RoasterOutput | null;
}

export interface RoasterSetup {
	device_id: string;
	kind: RoasterKind;
	control: ControlLevel;
	outputs: RoasterOutputs;
	/** While the heater is on, the fan never goes below this, %; 0: none. */
	fan_floor_pct: number;
	/** The extra values kept with a roast, drawn and usable in alarms (docs/channels.md). */
	channels: RoasterChannel[];
	/** The virtual roaster: always full, not changeable. */
	fixed: boolean;
	/** Null when the roaster has no setup yet (control off). */
	updated_at: string | null;
}

/** A change to a setup. Outputs and floor left out keep theirs, or take the new kind's
 * defaults when the kind changes (with its roast sequence and fan assist). */
export interface SetupChange {
	kind: RoasterKind;
	control: ControlLevel;
	outputs?: RoasterOutputs;
	fan_floor_pct?: number;
	/** Left out keeps the saved list. */
	channels?: RoasterChannel[];
}

/** A declared channel (docs/channels.md): an extra value the roaster sends, kept with a roast. */
export interface RoasterChannel {
	/** The sample's field name (`t3`, `exhaust`). */
	key: string;
	/** What the dashboard, alarms and Artisan call it, 1–24 characters. */
	label: string;
	/** `C` (a temperature, °C), `%`, or another short unit (its own scale). */
	unit: string;
	/** Top of the scale for another unit; null: the highest value seen. */
	max: number | null;
}

export const roasterSetup = {
	get: (deviceId: string) =>
		request<RoasterSetup>(`/api/roaster/${encodeURIComponent(deviceId)}/setup`),
	/** The roaster's latest message as it sent it (its field names): the channel picker's keys. */
	latest: (deviceId: string) =>
		request<{ device_id: string; timestamp: number; telemetry: Record<string, unknown> }>(
			`/api/roaster/${encodeURIComponent(deviceId)}/telemetry/latest`
		),
	/** The owner key; a token's change that leaves control on waits for the owner's approval (202). */
	set: (deviceId: string, change: SetupChange) =>
		request<RoasterSetup>(`/api/roaster/${encodeURIComponent(deviceId)}/setup`, {
			method: 'PUT',
			body: JSON.stringify(change)
		})
};

/** The server's last check of a roaster's gains after a boot. */
interface BootCheck {
	at: string;
	/** `server_start`: its first sample since the server started; `reboot`: its uptime went back. */
	why: string;
	reported?: Gains | null;
	held?: Gains | null;
	/** Null: it already had them, or the server holds none. */
	sent?: Gains | null;
}

export interface ContractView {
	device_id: string;
	firmware?: string | null;
	last_sample_s_ago?: number | null;
	reads: { bt: boolean; et: boolean; heater: boolean; fan: boolean };
	caps: DeviceCaps;
	gains: {
		reported?: Gains | null;
		held?: Gains | null;
		matches?: boolean | null;
		last_boot?: BootCheck | null;
	};
}

export const contract = {
	/** The roaster against the device contract (404 when not heard since the server started). */
	get: (deviceId: string) =>
		request<ContractView>(`/api/roaster/${encodeURIComponent(deviceId)}/contract`)
};
