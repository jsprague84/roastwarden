/**
 * How charts draw some lines, per browser (Settings → This browser), like the unit.
 *
 * `heater`: 'actual' draws every sample the roaster reported; 'smoothed' draws a 15 s
 * average (`lib/scope/smooth.ts`), since the PID's output swings by about 14 points a second
 * and its line can dominate the scope. Display only: nothing sent or stored changes.
 */

export type HeaterLine = 'actual' | 'smoothed';

const HEATER_KEY = 'roastwarden.chart.heater';

function load(): HeaterLine {
	try {
		const v = localStorage.getItem(HEATER_KEY);
		if (v === 'actual' || v === 'smoothed') return v;
	} catch {
		// private mode / storage disabled
	}
	return 'actual';
}

export const chartPrefs = $state<{ heater: HeaterLine }>({ heater: load() });

export function setHeaterLine(v: HeaterLine) {
	chartPrefs.heater = v;
	try {
		localStorage.setItem(HEATER_KEY, v);
	} catch {
		// ignore
	}
}
