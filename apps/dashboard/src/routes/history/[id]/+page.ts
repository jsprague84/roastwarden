import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { RoasterChannel } from '#lib/api/roasters.js';

export const load: PageLoad = async ({ params, fetch }) => {
	const [sessionRes, eventsRes, costRes] = await Promise.all([
		fetch(`/api/sessions/${params.id}`),
		fetch(`/api/sessions/${params.id}/events`),
		// null when not known: no bean, no green weight, no priced purchase before it.
		fetch(`/api/sessions/${params.id}/cost`)
	]);

	if (!sessionRes.ok) error(500, 'Failed to load session');
	if (!eventsRes.ok) error(500, 'Failed to load events');

	const [session, events] = await Promise.all([sessionRes.json(), eventsRes.json()]);
	// The roast's own declared channels, drawn with it (docs/channels.md); the server falls
	// back to the roaster's list for a roast from before they were kept.
	const channels: RoasterChannel[] = session.channels ?? [];
	return { session, events, cost: costRes.ok ? await costRes.json() : null, channels };
};
