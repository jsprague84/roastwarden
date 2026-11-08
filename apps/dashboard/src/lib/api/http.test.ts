import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { auth, adoptStoredKey, isOwnerCredential } from '#lib/api/auth.js';
import { control } from '#lib/api/control.js';
import { ControlOffError } from '#lib/api/http.js';
import { roasterSetup } from '#lib/api/roasters.js';
import { sessions } from '#lib/api/sessions.js';

// --- fetch mock ---

const mockFetch = vi.fn<typeof fetch>();
vi.stubGlobal('fetch', mockFetch);

function jsonResponse(data: unknown, status = 200): Response {
	return new Response(JSON.stringify(data), {
		status,
		headers: { 'Content-Type': 'application/json' }
	});
}

describe('API client', () => {
	beforeEach(() => {
		localStorage.clear();
		mockFetch.mockReset();
	});

	afterEach(() => {
		localStorage.clear();
	});

	describe('authorization', () => {
		it('sends no key: the login cookie goes with the request (same origin)', async () => {
			localStorage.setItem('roastwarden_api_key', 'test-secret-key');
			mockFetch.mockResolvedValueOnce(new Response(null, { status: 204 }));

			await control.setSetpoint('device1', 200);

			expect(mockFetch).toHaveBeenCalledOnce();
			const init = mockFetch.mock.calls[0][1];
			const headers = init?.headers as Record<string, string>;
			expect(headers['Authorization']).toBeUndefined();
			expect(init?.credentials).toBe('same-origin');
		});

		it('trades a key an older dashboard stored for a login, then forgets it', async () => {
			localStorage.setItem('roastwarden_api_key', 'owner-key-0123456789');
			mockFetch.mockResolvedValueOnce(
				jsonResponse({
					auth_enabled: true,
					reads_open: false,
					demo: false,
					credential: 'owner login',
					login_id: 1,
					token: null
				})
			);

			expect(await adoptStoredKey()).toBe(true);

			const [url, init] = mockFetch.mock.calls[0];
			expect(url).toBe('/api/auth/login');
			expect(JSON.parse(init?.body as string)).toEqual({ credential: 'owner-key-0123456789' });
			expect(localStorage.getItem('roastwarden_api_key')).toBeNull();
		});

		it('drops a stored key the server refuses, keeps it when the server is away', async () => {
			localStorage.setItem('roastwarden_api_key', 'old');
			mockFetch.mockRejectedValueOnce(new TypeError('Failed to fetch'));
			expect(await adoptStoredKey()).toBe(false);
			expect(localStorage.getItem('roastwarden_api_key')).toBe('old');

			mockFetch.mockResolvedValueOnce(
				new Response('{"error":{"code":"UNAUTHORIZED","message":"no"}}', { status: 401 })
			);
			expect(await adoptStoredKey()).toBe(false);
			expect(localStorage.getItem('roastwarden_api_key')).toBeNull();
			expect(await adoptStoredKey()).toBe(false);
			expect(mockFetch).toHaveBeenCalledTimes(2);
		});

		it('a refused sign-in says so', async () => {
			mockFetch.mockResolvedValueOnce(
				new Response('{"error":{"code":"UNAUTHORIZED","message":"That is not the owner key."}}', {
					status: 401
				})
			);
			await expect(auth.login('nope')).rejects.toThrow('That is not the owner key.');
		});

		it('knows the owner by key or login', () => {
			expect(isOwnerCredential('owner key')).toBe(true);
			expect(isOwnerCredential('owner login')).toBe(true);
			expect(isOwnerCredential('token')).toBe(false);
			expect(isOwnerCredential('open')).toBe(false);
		});

		it('handles 401 responses by dispatching auth_required event', async () => {
			mockFetch.mockResolvedValueOnce(new Response('Unauthorized', { status: 401 }));

			const authHandler = vi.fn();
			window.addEventListener('auth_required', authHandler);

			await expect(sessions.list()).rejects.toThrow('Authentication required');
			expect(authHandler).toHaveBeenCalledOnce();

			window.removeEventListener('auth_required', authHandler);
		});
	});

	describe('control endpoints', () => {
		it('sends setpoint with correct URL and body', async () => {
			mockFetch.mockResolvedValueOnce(new Response(null, { status: 204 }));

			await control.setSetpoint('roaster-1', 200);

			const [url, options] = mockFetch.mock.calls[0];
			expect(url).toBe('/api/roaster/roaster-1/control/setpoint');
			expect(options?.method).toBe('POST');
			expect(JSON.parse(options?.body as string)).toEqual({ value: 200 });
		});

		it('sends emergency stop with POST and no body', async () => {
			mockFetch.mockResolvedValueOnce(new Response(null, { status: 204 }));

			await control.emergencyStop('roaster-1');

			const [url, options] = mockFetch.mock.calls[0];
			expect(url).toBe('/api/roaster/roaster-1/control/emergency_stop');
			expect(options?.method).toBe('POST');
		});

		it("turns a 409 control_off into the server's message alone", async () => {
			const message =
				'control is off for esp32_x: Roastwarden only records it (Roasters → esp32_x → Control)';
			mockFetch.mockResolvedValueOnce(
				jsonResponse({ success: false, error: { code: 'CONTROL_OFF', message } }, 409)
			);

			const err = await control.setHeater('esp32_x', 50).catch((e) => e);

			expect(err).toBeInstanceOf(ControlOffError);
			expect(err.message).toBe(message);
		});

		it('throws ControlOffError when the output is not declared', async () => {
			const message = 'tc4 has no heater output (Roasters → tc4 → Control)';
			mockFetch.mockResolvedValueOnce(
				jsonResponse({ success: false, error: { code: 'OUTPUT_NOT_DECLARED', message } }, 409)
			);

			const err = await control.setHeater('tc4', 50).catch((e) => e);

			expect(err).toBeInstanceOf(ControlOffError);
			expect(err.message).toBe(message);
		});

		it('keeps the status on other errors', async () => {
			mockFetch.mockResolvedValueOnce(
				jsonResponse({ success: false, error: { code: 'CONFLICT', message: 'busy' } }, 409)
			);

			const err = await control.setHeater('esp32_x', 50).catch((e) => e);

			expect(err).not.toBeInstanceOf(ControlOffError);
			expect(err.message).toBe('API error 409: busy');
		});
	});

	describe('roaster setup', () => {
		it('reads and writes the setup', async () => {
			const body = {
				device_id: 'esp32_x',
				kind: 'drum_gas',
				control: 'manual',
				fixed: false,
				updated_at: '2026-10-31T00:00:00Z'
			};
			mockFetch.mockResolvedValueOnce(jsonResponse(body));
			mockFetch.mockResolvedValueOnce(jsonResponse(body));

			await roasterSetup.get('esp32_x');
			const saved = await roasterSetup.set('esp32_x', { kind: 'drum_gas', control: 'manual' });

			expect(mockFetch.mock.calls[0][0]).toBe('/api/roaster/esp32_x/setup');
			const [url, options] = mockFetch.mock.calls[1];
			expect(url).toBe('/api/roaster/esp32_x/setup');
			expect(options?.method).toBe('PUT');
			expect(JSON.parse(options?.body as string)).toEqual({ kind: 'drum_gas', control: 'manual' });
			expect(saved.control).toBe('manual');
		});
	});

	describe('session endpoints', () => {
		it('creates a session with POST', async () => {
			const session = {
				id: '1',
				name: 'Test Roast',
				device_id: 'dev1',
				status: 'created',
				created_at: '2026-01-01T00:00:00Z'
			};
			mockFetch.mockResolvedValueOnce(jsonResponse(session));

			const result = await sessions.create({ name: 'Test Roast', device_id: 'dev1' });

			const [url, options] = mockFetch.mock.calls[0];
			expect(url).toBe('/api/sessions');
			expect(options?.method).toBe('POST');
			expect(result.name).toBe('Test Roast');
		});

		it('lists sessions with optional query params', async () => {
			mockFetch.mockResolvedValueOnce(jsonResponse([]));

			await sessions.list('dev1', 10);

			const [url] = mockFetch.mock.calls[0];
			expect(url).toBe('/api/sessions?device_id=dev1&limit=10');
		});
	});
});

describe('errorMessage', () => {
	it('reads the API error body and passes plain text through', async () => {
		const { errorMessage } = await import('#lib/api/http.js');
		expect(
			errorMessage('{"success":false,"error":{"code":"BAD_REQUEST","message":"Keep 0 to 365"}}')
		).toBe('Keep 0 to 365');
		expect(errorMessage(' Bad Gateway \n')).toBe('Bad Gateway');
		expect(errorMessage('{"other":1}')).toBe('{"other":1}');
	});
});
