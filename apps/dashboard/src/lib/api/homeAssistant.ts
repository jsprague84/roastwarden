/** Home Assistant (docs/home-assistant.md; Settings → Home Assistant). */

import { request } from '#lib/api/http.js';

interface HaRoaster {
	device_id: string;
	name: string;
	/** Home Assistant shows it available (it reports) */
	available: boolean;
	config_topic: string;
	state_topic: string;
	entities: string[];
}

export interface HaStatus {
	enabled: boolean;
	/** 'server': the server's own broker; else host:port of Home Assistant's */
	broker: string;
	username: string | null;
	password_set: boolean;
	connected: boolean;
	prefix: string;
	include_virtual: boolean;
	interval_seconds: number;
	/** The user Home Assistant signs in to the built-in broker as */
	built_in_user: string | null;
	/** The built-in broker runs without a Home Assistant user (MQTT_HOME_ASSISTANT_PASSWORD) */
	built_in_without_user: boolean;
	roasters: HaRoaster[];
	/** Heard, but waiting to be set up (Roasters): not announced until then */
	waiting: string[];
	last_published_at: string | null;
	problem: string | null;
}

export const homeAssistantApi = {
	status: () => request<HaStatus>('/api/home-assistant')
};
