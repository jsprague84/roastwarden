/** Words for what the fan assist is doing (docs/fan-assist.md). */
import type { FanAssistView, AssistView } from '#lib/api/fanAssist.js';

const STATUS: Record<AssistView['status'], string> = {
	waiting: 'waiting for BT to reach the arming temperature',
	no_data: 'no heater reading: holding',
	at_ceiling: 'fan at your setting',
	holding: 'holding',
	lowering: 'heater pinned: lowering the fan',
	raising: 'heater has room: giving fan back',
	at_floor: 'at the floor, heater still pinned'
};

export function statusText(s: AssistView['status']): string {
	return STATUS[s] ?? s;
}

/** One line for the control panel, or null when there is nothing to say. */
export function assistSummary(v: FanAssistView | null): string | null {
	if (!v) return null;
	if (!v.config.enabled) return null;
	const st = v.state;
	const a = st.view;
	if (!st.active || !a) return `Fan assist on · ${st.reason}`;
	const trim = Math.round(a.trim);
	const head = trim >= 1 ? `Fan assist −${trim} % → ${Math.round(a.fan)} %` : 'Fan assist on';
	const heater = a.heater != null ? ` · heater ${Math.round(a.heater)} %` : '';
	const lag =
		a.lag != null && Math.abs(a.lag) >= 1
			? ` · BT ${Math.abs(a.lag).toFixed(0)} °C ${a.lag > 0 ? 'behind' : 'ahead'}`
			: '';
	return `${head} · ${statusText(a.status)}${heater}${lag}`;
}

/** Whether the assist has taken any fan away. */
export function isTrimming(v: FanAssistView | null): boolean {
	return !!v?.state.active && (v.state.view?.trim ?? 0) >= 1;
}
