/**
 * A public demo (docs/DEPLOY.md, 'A public demo'): whether this server is one, the visitor's
 * own simulated roaster, and the visit counting. On any other server `demo` stays false and
 * nothing here loads or sends anything.
 */
import { auth } from '#lib/api/auth.js';
import { demo as demoApi } from '#lib/api/demo.js';

export const demoState = $state({
	/** The server is a public demo. */
	demo: false,
	/** The visitor's own simulated roaster, once the server gave one. */
	roaster: null as string | null,
	/** The demo had no roaster to spare: the visitor shares `virtual_roaster`. */
	shared: false
});

let loading: Promise<boolean> | null = null;

/** Ask the server once whether it is a demo; on one that counts visits, load the tracker. */
export function loadDemo(): Promise<boolean> {
	loading ??= auth
		.status()
		.then((s) => {
			demoState.demo = s.demo;
			if (s.demo && s.demo_stats) loadTracker(s.demo_stats.url, s.demo_stats.website);
			return s.demo;
		})
		.catch(() => false);
	return loading;
}

let claiming: Promise<string | null> | null = null;

/**
 * On a demo, this browser's own simulated roaster (the server makes it on the first call and
 * sets a cookie); null on any other server. The shared `virtual_roaster` when the demo is full.
 */
export function claimRoaster(): Promise<string | null> {
	claiming ??= loadDemo().then(async (isDemo) => {
		if (!isDemo) return null;
		try {
			const r = await demoApi.roaster();
			demoState.roaster = r.device_id;
			demoState.shared = r.shared;
			return r.device_id;
		} catch {
			return null;
		}
	});
	return claiming;
}

declare global {
	interface Window {
		umami?: { track: (event: string, data?: Record<string, string | number>) => void };
	}
}

/** Umami's script, as roastwarden.com loads it: no cookies, Do Not Track honoured. */
function loadTracker(url: string, website: string) {
	if (typeof document === 'undefined' || document.querySelector('script[data-website-id]')) return;
	const s = document.createElement('script');
	s.defer = true;
	s.src = `${url}/script.js`;
	s.dataset.websiteId = website;
	s.dataset.doNotTrack = 'true';
	document.head.appendChild(s);
}

/** Count something a demo visitor did ("Roast started"); nothing on any other server. */
export function track(event: string, data?: Record<string, string | number>) {
	if (!demoState.demo) return;
	try {
		window.umami?.track(event, data);
	} catch {
		/* counting never gets in the way */
	}
}
