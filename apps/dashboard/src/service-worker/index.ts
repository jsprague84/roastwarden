// The installable app (docs/push.md): serves the app shell's hashed assets from a cache, and
// shows alarm notifications pushed by the server. Nothing dynamic is cached: `/api/`, `/ws/`,
// the manifest and page navigations always go to the network, so a stale worker can never
// show stale roast data or step around the edge login. Each build gets its own cache; older
// ones are deleted on activate.

import { version } from '$app/env';
import { assets, immutable } from '$app/manifest';
import { self as sw } from '$app/service-worker';

const CACHE = `roastwarden-${version}`;
const ASSETS = new Set<string>(
	[...immutable, ...assets]
		.map((a) => new URL(a.path, sw.location.href).pathname)
		.filter((p) => !p.endsWith('/manifest.webmanifest'))
);

sw.addEventListener('install', (event) => {
	event.waitUntil(
		caches
			.open(CACHE)
			.then((cache) => cache.addAll([...ASSETS]))
			.then(() => sw.skipWaiting())
	);
});

sw.addEventListener('activate', (event) => {
	event.waitUntil(
		caches
			.keys()
			.then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
			.then(() => sw.clients.claim())
	);
});

sw.addEventListener('fetch', (event) => {
	if (event.request.method !== 'GET') return;
	const url = new URL(event.request.url);
	if (url.origin !== sw.location.origin || !ASSETS.has(url.pathname)) return;
	event.respondWith(
		caches.open(CACHE).then(async (cache) => {
			const hit = await cache.match(event.request);
			if (hit) return hit;
			const response = await fetch(event.request);
			if (response.ok) cache.put(event.request, response.clone());
			return response;
		})
	);
});

interface PushMessage {
	title: string;
	body: string;
	tag: string;
	url: string;
}

// An alarm from the server (crates/server/src/notify/push.rs, `Message`).
sw.addEventListener('push', (event) => {
	let msg: PushMessage = { title: 'Roastwarden', body: '', tag: 'roastwarden', url: '/' };
	try {
		msg = { ...msg, ...(event.data?.json() as Partial<PushMessage>) };
	} catch {
		msg.body = event.data?.text() ?? '';
	}
	event.waitUntil(
		sw.registration.showNotification(msg.title, {
			body: msg.body,
			tag: msg.tag,
			icon: '/icons/icon-192.png',
			badge: '/icons/icon-192.png',
			data: { url: msg.url },
			requireInteraction: msg.tag !== 'test'
		})
	);
});

// A tap opens the dashboard, reusing a window that is already open.
sw.addEventListener('notificationclick', (event) => {
	event.notification.close();
	const url = new URL(
		(event.notification.data as { url?: string } | null)?.url ?? '/',
		sw.location.origin
	).href;
	event.waitUntil(
		sw.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((windows) => {
			const open = windows.find((w) => new URL(w.url).origin === sw.location.origin);
			if (open) return open.focus().then((w) => w.navigate(url));
			return sw.clients.openWindow(url);
		})
	);
});
