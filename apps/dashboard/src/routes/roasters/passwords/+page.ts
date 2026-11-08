import type { PageLoad } from './$types';
import type { MqttPasswordList } from '#lib/roaster/mqttPasswords.js';

export const load: PageLoad = async ({ fetch }) => {
	const res = await fetch('/api/mqtt/passwords');
	if (!res.ok) return { list: null as MqttPasswordList | null, status: res.status };
	return { list: (await res.json()) as MqttPasswordList, status: res.status };
};
