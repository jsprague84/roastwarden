/**
 * A public demo's visitor and their own simulated roaster (docs/DEPLOY.md, "A simulated roaster
 * per visitor").
 */

import { request } from '#lib/api/http.js';

/** A demo visitor's own simulated roaster (docs/DEPLOY.md, "A simulated roaster per visitor"). */
export interface DemoRoaster {
	/** The roaster to select: the visitor's own, or `virtual_roaster` when the demo is full */
	device_id: string;
	/** The visitor shares `virtual_roaster` with everyone */
	shared: boolean;
	idle_minutes: number;
}

export const demo = {
	/** On a public demo: this browser's own simulated roaster (made on the first call). */
	roaster: () => request<DemoRoaster>('/api/demo/roaster', { method: 'POST' })
};
