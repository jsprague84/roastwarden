/** Fan assist (docs/fan-assist.md). */

import { request, roasterPath } from '#lib/api/http.js';

export interface FanAssistConfig {
	enabled: boolean;
	/** The assist never trims the fan below this, %. */
	min_fan_pct: number;
	/** Acts once BT reaches this, °C. */
	arm_bt_c: number;
	/** Filtered heater above this lowers the fan, %. */
	high_heater_pct: number;
	/** Filtered heater below this gives fan back, %. */
	low_heater_pct: number;
	max_down_pct_per_min: number;
	max_up_pct_per_min: number;
	/** Extra trim rate per °C BT is behind (giving back: ahead of) its target, %/min per °C. */
	lag_gain_pct_per_min_per_c: number;
}

type AssistStatus =
	'waiting' | 'no_data' | 'at_ceiling' | 'holding' | 'lowering' | 'raising' | 'at_floor';

export interface AssistView {
	status: AssistStatus;
	/** Fan the operator or profile asks for, %. */
	ceiling: number;
	floor: number;
	/** Fan taken away, %. */
	trim: number;
	/** Fan the assist commands, %. */
	fan: number;
	/** Filtered heater output, %. */
	heater: number | null;
	/** BT behind its target, °C (negative: ahead). */
	lag?: number | null;
	armed: boolean;
}

export interface FanAssistView {
	device_id: string;
	config: FanAssistConfig;
	state: {
		/** replay: a replay runs it (profile fan = ceiling); manual: operator's fan = ceiling. */
		source: 'replay' | 'manual' | null;
		active: boolean;
		reason: string;
		view?: AssistView | null;
	};
}

export const fanAssist = {
	get: (deviceId: string) => request<FanAssistView>(`${roasterPath(deviceId)}/fan_assist`),
	put: (deviceId: string, config: FanAssistConfig) =>
		request<FanAssistView>(
			`${roasterPath(deviceId)}/fan_assist`,
			{ method: 'PUT', body: JSON.stringify(config) },
			true
		)
};
