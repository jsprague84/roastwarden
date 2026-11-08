/**
 * The modules that are on (docs/lean-home.md), from `GET /api/about`. Until it answers, the
 * list last seen in this browser (else every module, as on an install from before modules).
 */
import { about } from '#lib/api/about.js';
import { settings } from '#lib/api/settings.js';
import { MODULES, normalize, toValue, type Module } from '#lib/modules/modules.js';

const STORAGE_KEY = 'roastwarden.modules';

function cached(): Module[] {
	try {
		const v = localStorage.getItem(STORAGE_KEY);
		if (v) return normalize(JSON.parse(v));
	} catch {
		// storage off or a bad value: every module
	}
	return [...MODULES];
}

function remember(on: Module[]) {
	try {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(on));
	} catch {
		// ignore
	}
}

export const modulesState = $state<{ on: Module[]; firstRun: boolean }>({
	on: cached(),
	firstRun: false
});

export function isOn(m: Module): boolean {
	return modulesState.on.includes(m);
}

/** Read them from the server; null when it did not answer (the cached list stays). */
export async function loadModules(): Promise<{ firstRun: boolean } | null> {
	try {
		const a = await about.get();
		modulesState.on = normalize(a.modules);
		modulesState.firstRun = a.first_run;
		remember(modulesState.on);
		return { firstRun: a.first_run };
	} catch {
		return null;
	}
}

/** Save a list as the server's setting. */
export async function saveModules(on: readonly Module[]): Promise<void> {
	await settings.set('modules', toValue(on));
	modulesState.on = normalize(on);
	modulesState.firstRun = false;
	remember(modulesState.on);
}
