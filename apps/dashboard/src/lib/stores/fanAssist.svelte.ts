/**
 * Shared fan assist state for the device (docs/fan-assist.md). Components
 * call `watchFanAssist` from an effect; while anything watches, the state
 * is polled every 2 s.
 */
import { fanAssist, type FanAssistConfig, type FanAssistView } from '#lib/api/fanAssist.js';

export const fanAssistState = $state<{
	deviceId: string | null;
	view: FanAssistView | null;
	error: string | null;
}>({
	deviceId: null,
	view: null,
	error: null
});

const POLL_MS = 2000;
let watchers = 0;
let timer: ReturnType<typeof setInterval> | null = null;

async function refresh() {
	const id = fanAssistState.deviceId;
	if (!id) return;
	try {
		const v = await fanAssist.get(id);
		if (fanAssistState.deviceId === id) {
			fanAssistState.view = v;
			fanAssistState.error = null;
		}
	} catch (e) {
		fanAssistState.error = e instanceof Error ? e.message : String(e);
	}
}

/** Watch a device's fan assist; returns the cleanup for an effect. */
export function watchFanAssist(deviceId: string): () => void {
	if (fanAssistState.deviceId !== deviceId) {
		fanAssistState.deviceId = deviceId;
		fanAssistState.view = null;
	}
	watchers += 1;
	refresh();
	if (!timer) timer = setInterval(refresh, POLL_MS);
	return () => {
		watchers -= 1;
		if (watchers <= 0 && timer) {
			clearInterval(timer);
			timer = null;
			watchers = 0;
		}
	};
}

export async function saveFanAssist(
	deviceId: string,
	config: FanAssistConfig
): Promise<FanAssistView> {
	const v = await fanAssist.put(deviceId, config);
	if (fanAssistState.deviceId === deviceId) fanAssistState.view = v;
	return v;
}
