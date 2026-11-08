/**
 * The simulated roasters (docs/virtual-roaster.md): the fluid bed, the drum, a demo visitor's own.
 */

import { request } from '#lib/api/http.js';

/** The simulated drum roaster (`virtual_drum`, docs/add-roaster.md): °C and %. */
interface VirtualDrumState {
	enabled: boolean;
	device_id: string;
	/** Beans in the drum. */
	loaded: boolean;
	bt: number;
	exhaust: number;
	inlet: number;
	burner: number;
	air: number;
	drum: number;
}

export interface VirtualRoasterState {
	enabled: boolean;
	device_id: string;
	/** Beans in the virtual chamber. */
	loaded: boolean;
	bt: number;
	et: number;
	heater_applied: number;
	fan: number;
	/** The simulated drum beside it (absent from an older server). */
	drum?: VirtualDrumState;
}

/** `?roaster=virtual_drum` for the drum (or a demo visitor's own roaster); nothing for the shared fluid bed. */
function simQuery(roaster?: string): string {
	return roaster && roaster !== 'virtual_roaster' ? `?roaster=${encodeURIComponent(roaster)}` : '';
}

export const virtualRoaster = {
	/** The shared simulator's state (and the drum's), or with `roaster` a demo visitor's own. */
	get: (roaster?: string) =>
		request<VirtualRoasterState>(`/api/virtual-roaster${simQuery(roaster)}`),
	setEnabled: (enabled: boolean) =>
		request<VirtualRoasterState>(
			'/api/virtual-roaster',
			{ method: 'PUT', body: JSON.stringify({ enabled }) },
			true
		),
	/** Switch the simulated drum on or off (the first time it sets up its roaster). */
	setDrum: (on: boolean) =>
		request<VirtualRoasterState>(
			'/api/virtual-roaster',
			{ method: 'PUT', body: JSON.stringify({ drum: on }) },
			true
		),
	/** Cold beans in: the fluid bed's chamber, or the drum's with `virtual_drum`. */
	charge: (roaster?: string) =>
		request<VirtualRoasterState>(
			`/api/virtual-roaster/charge${simQuery(roaster)}`,
			{ method: 'POST' },
			true
		),
	dump: (roaster?: string) =>
		request<VirtualRoasterState>(
			`/api/virtual-roaster/dump${simQuery(roaster)}`,
			{ method: 'POST' },
			true
		)
};
