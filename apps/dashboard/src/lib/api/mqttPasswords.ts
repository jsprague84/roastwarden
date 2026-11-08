/** Roasters' MQTT passwords on the built-in broker (docs/add-roaster.md, "MQTT passwords"). */

import { request } from '#lib/api/http.js';

type MqttPasswordGiven = import('#lib/roaster/mqttPasswords.js').MqttPasswordGiven;

export const mqttPasswords = {
	list: () =>
		request<import('#lib/roaster/mqttPasswords.js').MqttPasswordList>('/api/mqtt/passwords'),
	/** A new roaster's password (none given: a random one); answered once */
	add: (roasterId: string, password?: string) =>
		request<MqttPasswordGiven>('/api/mqtt/passwords', {
			method: 'POST',
			body: JSON.stringify(
				password ? { roaster_id: roasterId, password } : { roaster_id: roasterId }
			)
		}),
	/** A new password: signs the roaster out until its sketch has it */
	change: (roasterId: string, password?: string) =>
		request<MqttPasswordGiven>(`/api/mqtt/passwords/${encodeURIComponent(roasterId)}`, {
			method: 'PUT',
			body: JSON.stringify(password ? { password } : {})
		}),
	remove: (roasterId: string) =>
		request<void>(`/api/mqtt/passwords/${encodeURIComponent(roasterId)}`, { method: 'DELETE' })
};
