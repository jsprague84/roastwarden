<script lang="ts">
	/**
	 * Alarm notifications (docs/push.md): turn Web Push on for this device, send a test, and
	 * see every device that gets alarms. Pushes come from the server, so they arrive with the
	 * dashboard closed and the phone locked.
	 */
	import { push, type PushSubscriptionRow } from '#lib/api/push.js';
	import { deviceName, keyBytes, pushSupport, type PushSupport } from '#lib/push/browser.js';
	import { ago } from '#lib/tokens/state.js';
	import { notifications } from '#lib/stores/notifications.js';

	let support = $state<PushSupport>('ok');
	let permission = $state<NotificationPermission>('default');
	let mine = $state<PushSubscription | null>(null);
	let rows = $state<PushSubscriptionRow[]>([]);
	let busy = $state(false);

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	/** The service worker, or an error if the app has none (it registers on load). */
	async function worker(): Promise<ServiceWorkerRegistration> {
		const timeout = new Promise<never>((_, no) =>
			setTimeout(
				() => no(new Error('the app’s service worker did not start; reload the page')),
				8000
			)
		);
		return Promise.race([navigator.serviceWorker.ready, timeout]);
	}

	async function load() {
		support = pushSupport(window as unknown as Parameters<typeof pushSupport>[0]);
		if (support === 'ok') {
			permission = Notification.permission;
			try {
				mine = await (await worker()).pushManager.getSubscription();
			} catch {
				mine = null;
			}
		}
		try {
			rows = await push.list();
		} catch (e) {
			notifications.add(`Could not read the notification devices: ${msg(e)}`, 'error');
		}
	}
	$effect(() => {
		void load();
	});

	async function turnOn() {
		busy = true;
		try {
			permission = await Notification.requestPermission();
			if (permission !== 'granted') return;
			const reg = await worker();
			const { public_key } = await push.key();
			// A subscription made with another server key cannot be reused.
			await (await reg.pushManager.getSubscription())?.unsubscribe();
			const sub = await reg.pushManager.subscribe({
				userVisibleOnly: true,
				applicationServerKey: keyBytes(public_key)
			});
			await push.subscribe(sub.toJSON(), deviceName(navigator.userAgent));
			mine = sub;
			notifications.add('Alarm notifications are on for this device.', 'success');
		} catch (e) {
			notifications.add(`Could not turn notifications on: ${msg(e)}`, 'error');
		} finally {
			busy = false;
			await load();
		}
	}

	async function turnOff() {
		if (!mine) return;
		busy = true;
		try {
			const endpoint = mine.endpoint;
			await mine.unsubscribe();
			await push.unsubscribe(endpoint);
			mine = null;
		} catch (e) {
			notifications.add(`Could not turn notifications off: ${msg(e)}`, 'error');
		} finally {
			busy = false;
			await load();
		}
	}

	async function test(id?: number) {
		try {
			const d = await push.test(id);
			const bad = d.filter((x) => !x.ok);
			if (!d.length) notifications.add('No device gets notifications yet.', 'warning');
			else if (!bad.length)
				notifications.add(
					`Test sent to ${d.length} device${d.length === 1 ? '' : 's'}.`,
					'success'
				);
			else
				notifications.add(
					`Not delivered to ${bad.map((x) => `${x.name} (${x.detail})`).join(', ')}`,
					'error'
				);
		} catch (e) {
			notifications.add(`Test failed: ${msg(e)}`, 'error');
		}
		await load();
	}

	async function remove(r: PushSubscriptionRow) {
		try {
			await push.remove(r.id);
		} catch (e) {
			notifications.add(`Could not remove: ${msg(e)}`, 'error');
		}
		await load();
	}

	const primary =
		'rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
</script>

<h2 class="text-sm font-semibold text-foreground">Alarm notifications</h2>
<p class="mt-1 text-sm text-muted-foreground">
	Alarms reach this device as notifications, also with the dashboard closed and the screen locked.
	The server sends them to every device turned on here.
</p>

<div class="mt-3" data-testid="push-state">
	{#if support === 'insecure'}
		<p class="text-sm text-foreground">Needs the dashboard over https.</p>
	{:else if support === 'needs-install'}
		<p class="text-sm text-foreground">
			On an iPhone or iPad, add Roastwarden to the home screen (Share → Add to Home Screen), open it
			from there, and turn notifications on in the app.
		</p>
	{:else if support === 'unsupported'}
		<p class="text-sm text-foreground">This browser cannot receive push notifications.</p>
	{:else if permission === 'denied'}
		<p class="text-sm text-foreground">
			Notifications are blocked for this site; allow them in the browser’s site settings, then come
			back.
		</p>
	{:else if mine}
		<div class="flex flex-wrap items-center gap-2">
			<span class="text-sm text-foreground">On for this device.</span>
			<button class={secondary} onclick={() => test()} disabled={busy}>Send a test</button>
			<button class={secondary} onclick={turnOff} disabled={busy}>Turn off</button>
		</div>
	{:else}
		<button class={primary} onclick={turnOn} disabled={busy}>Turn on for this device</button>
	{/if}
</div>

{#if rows.length}
	<h3 class="mt-4 text-xs font-medium uppercase tracking-wide text-muted-foreground">
		Devices that get alarms
	</h3>
	<ul class="mt-1 divide-y divide-border text-sm">
		{#each rows as r (r.id)}
			<li class="flex items-center gap-3 py-2">
				<div class="min-w-0 flex-1">
					<p class="truncate text-foreground">{r.name}</p>
					<p
						class="text-xs [overflow-wrap:anywhere] {r.last_error
							? 'text-red-400'
							: 'text-muted-foreground'}"
					>
						{r.last_error
							? `Last try failed: ${r.last_error}`
							: r.last_ok_at
								? `Delivered ${ago(r.last_ok_at)}`
								: 'Nothing sent yet'} · {r.service}
					</p>
				</div>
				<button class="{secondary} shrink-0 py-1 text-xs" onclick={() => test(r.id)}>Test</button>
				<button class="{secondary} shrink-0 py-1 text-xs" onclick={() => remove(r)}>Remove</button>
			</li>
		{/each}
	</ul>
{/if}
