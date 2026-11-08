/** Pieces of turning alarm notifications on in this browser (docs/push.md). */

/** The server's base64url public key as `applicationServerKey` wants it. */
export function keyBytes(base64url: string): Uint8Array<ArrayBuffer> {
	const b64 = base64url.replace(/-/g, '+').replace(/_/g, '/');
	const bin = atob(b64 + '='.repeat((4 - (b64.length % 4)) % 4));
	const out = new Uint8Array(new ArrayBuffer(bin.length));
	for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
	return out;
}

/** "Chrome on Android" and the like, to tell this device apart in the list. */
export function deviceName(ua: string): string {
	const browser = /Edg\//.test(ua)
		? 'Edge'
		: /Firefox\//.test(ua)
			? 'Firefox'
			: /Chrome\//.test(ua)
				? 'Chrome'
				: /Safari\//.test(ua)
					? 'Safari'
					: 'Browser';
	const os = /iPhone/.test(ua)
		? 'iPhone'
		: /iPad/.test(ua)
			? 'iPad'
			: /Android/.test(ua)
				? 'Android'
				: /Mac OS X/.test(ua)
					? 'macOS'
					: /Windows/.test(ua)
						? 'Windows'
						: /Linux/.test(ua)
							? 'Linux'
							: '';
	return os ? `${browser} on ${os}` : browser;
}

export type PushSupport = 'ok' | 'unsupported' | 'needs-install' | 'insecure';

/**
 * Whether this browser can get pushes. iOS and iPadOS deliver them only to an app added to
 * the home screen; plain http (other than localhost) has no service workers.
 */
export function pushSupport(w: {
	isSecureContext: boolean;
	navigator: { userAgent: string; serviceWorker?: unknown; standalone?: boolean };
	PushManager?: unknown;
	matchMedia?: (q: string) => { matches: boolean };
}): PushSupport {
	if (!w.isSecureContext) return 'insecure';
	const ios = /iPhone|iPad/.test(w.navigator.userAgent);
	const installed =
		w.navigator.standalone === true ||
		(w.matchMedia?.('(display-mode: standalone)').matches ?? false);
	if (ios && !installed) return 'needs-install';
	if (!w.navigator.serviceWorker || !w.PushManager) return 'unsupported';
	return 'ok';
}
