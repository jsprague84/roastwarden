import { describe, expect, it } from 'vitest';
import { deviceName, keyBytes, pushSupport } from './browser.js';

describe('keyBytes', () => {
	it('decodes base64url without padding', () => {
		expect([...keyBytes('BP4z_-8')]).toEqual([0x04, 0xfe, 0x33, 0xff, 0xef]);
		expect(keyBytes('AAAA').length).toBe(3);
	});
});

describe('deviceName', () => {
	it('names browser and system', () => {
		expect(
			deviceName(
				'Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Mobile Safari/537.36'
			)
		).toBe('Chrome on Android');
		expect(
			deviceName(
				'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1'
			)
		).toBe('Safari on iPhone');
		expect(deviceName('curl/8')).toBe('Browser');
	});
});

describe('pushSupport', () => {
	const ua = {
		iphone: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1',
		linux: 'Mozilla/5.0 (X11; Linux x86_64) Chrome/141.0'
	};
	it('needs https', () => {
		expect(
			pushSupport({
				isSecureContext: false,
				navigator: { userAgent: ua.linux, serviceWorker: {} },
				PushManager: {}
			})
		).toBe('insecure');
	});
	it('needs the home-screen app on an iPhone', () => {
		expect(
			pushSupport({
				isSecureContext: true,
				navigator: { userAgent: ua.iphone, serviceWorker: {} },
				PushManager: {}
			})
		).toBe('needs-install');
		expect(
			pushSupport({
				isSecureContext: true,
				navigator: { userAgent: ua.iphone, serviceWorker: {}, standalone: true },
				PushManager: {}
			})
		).toBe('ok');
	});
	it('needs a service worker and PushManager', () => {
		expect(pushSupport({ isSecureContext: true, navigator: { userAgent: ua.linux } })).toBe(
			'unsupported'
		);
		expect(
			pushSupport({
				isSecureContext: true,
				navigator: { userAgent: ua.linux, serviceWorker: {} },
				PushManager: {}
			})
		).toBe('ok');
	});
});
