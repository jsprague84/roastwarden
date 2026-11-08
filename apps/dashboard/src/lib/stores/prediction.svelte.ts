/**
 * The live prediction from the roaster's own model (docs/prediction.md), polled every 3 s
 * while anything watches it (the scope, with its "BT model" series on). The server makes it
 * every 5 s from CHARGE + 30 s to DROP; outside that the state is null.
 */
import { prediction, type Prediction } from '#lib/api/predict.js';

export const predictionState = $state<{ deviceId: string | null; live: Prediction | null }>({
	deviceId: null,
	live: null
});

const POLL_MS = 3000;
let watchers = 0;
let timer: ReturnType<typeof setInterval> | null = null;

async function refresh() {
	const id = predictionState.deviceId;
	if (!id) return;
	try {
		const p = await prediction.live(id);
		if (predictionState.deviceId === id) predictionState.live = p;
	} catch {
		// 404 outside a roast's CHARGE + 30 s … DROP
		if (predictionState.deviceId === id) predictionState.live = null;
	}
}

/** Watch a roaster's live prediction; returns the cleanup for an effect. */
export function watchPrediction(deviceId: string): () => void {
	if (predictionState.deviceId !== deviceId) {
		predictionState.deviceId = deviceId;
		predictionState.live = null;
	}
	watchers += 1;
	refresh();
	if (!timer) timer = setInterval(refresh, POLL_MS);
	return () => {
		watchers -= 1;
		if (watchers <= 0) {
			if (timer) clearInterval(timer);
			timer = null;
			watchers = 0;
			predictionState.live = null;
		}
	};
}
