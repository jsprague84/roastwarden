<script lang="ts">
	/**
	 * Notifications (docs/notifications.md): where alarms and system events go. Web Push is
	 * set up per browser (This browser); Gotify, ntfy, Matrix and a webhook are set up here,
	 * each with its own Save and a Test of the values as typed.
	 */
	import { resolve } from '$app/paths';
	import {
		notifyApi,
		type NotificationOverview,
		type NotifyChannel
	} from '#lib/api/notifications.js';
	import { settings } from '#lib/api/settings.js';
	import {
		CHANNELS,
		channelValues,
		fromOverview,
		missing,
		statusLine,
		type ChannelDef,
		type FormState
	} from '#lib/notify/channels.js';
	import { notifications } from '#lib/stores/notifications.js';

	let view = $state<NotificationOverview | null>(null);
	let form = $state<FormState>({ enabled: {}, values: {}, isSet: {} });
	let busy = $state<Record<string, boolean>>({});
	let result = $state<Record<string, { ok: boolean; text: string }>>({});
	let events = $state({ backup: true, offline: true, stock: true, offline_minutes: 3 });

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	async function load() {
		try {
			view = await notifyApi.overview();
			form = fromOverview(view);
			events = { ...view.events };
		} catch (e) {
			notifications.add(`Could not read the notification settings: ${msg(e)}`, 'error');
		}
	}
	$effect(() => {
		void load();
	});

	async function save(ch: ChannelDef) {
		busy = { ...busy, [ch.id]: true };
		try {
			await settings.setMany(channelValues(ch, form));
			result = {
				...result,
				[ch.id]: { ok: true, text: form.enabled[ch.id] ? 'Saved: on.' : 'Saved: off.' }
			};
			const keepTyping = { ...form.values };
			await load();
			// Secrets come back empty; anything else typed is now stored.
			for (const f of ch.fields) if (!f.secret) form.values[f.key] = keepTyping[f.key];
		} catch (e) {
			result = { ...result, [ch.id]: { ok: false, text: msg(e) } };
		} finally {
			busy = { ...busy, [ch.id]: false };
		}
	}

	async function test(id: NotifyChannel, values: Record<string, string> = {}) {
		busy = { ...busy, [`test-${id}`]: true };
		try {
			const r = await notifyApi.test(id, values);
			result = { ...result, [id]: { ok: true, text: r.message } };
		} catch (e) {
			result = { ...result, [id]: { ok: false, text: msg(e) } };
		} finally {
			busy = { ...busy, [`test-${id}`]: false };
			view = await notifyApi.overview().catch(() => view);
		}
	}

	async function forget(ch: ChannelDef, key: string) {
		try {
			await settings.set(key, '');
			form.isSet[key] = false;
			result = { ...result, [ch.id]: { ok: true, text: 'Removed.' } };
		} catch (e) {
			result = { ...result, [ch.id]: { ok: false, text: msg(e) } };
		}
	}

	async function saveEvents() {
		const minutes = Math.max(1, Math.min(1440, Math.round(Number(events.offline_minutes) || 3)));
		try {
			await settings.setMany({
				notify_event_backup: String(events.backup),
				notify_event_offline: String(events.offline),
				notify_event_stock: String(events.stock),
				notify_offline_minutes: String(minutes)
			});
			result = { ...result, events: { ok: true, text: 'Saved.' } };
			await load();
		} catch (e) {
			result = { ...result, events: { ok: false, text: msg(e) } };
		}
	}

	const when = (iso: string) =>
		new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
	const input =
		'w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const num =
		'w-20 rounded-md border border-border bg-input px-2 py-1 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus disabled:opacity-50';
	const primary =
		'rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50';
	const secondary =
		'rounded-md border border-border px-3 py-1.5 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50';
</script>

{#snippet outcome(id: string)}
	{@const r = result[id]}
	{#if r}<p class="mt-2 text-sm {r.ok ? 'text-green-400' : 'text-red-400'}" role="status">
			{r.text}
		</p>{/if}
{/snippet}

<p class="text-sm text-muted-foreground">
	Every fired alarm and every system event goes to each channel that is on. Web Push needs nothing
	set up here; the others need an account somewhere.
</p>

{#if view?.problems.length}
	<div class="mt-4 border-l-2 border-amber-500 pl-3 text-sm" role="status">
		<p class="font-medium text-foreground">Not cleared yet</p>
		<ul class="mt-1 space-y-0.5">
			{#each view.problems as p (p.key)}
				<li class="text-foreground">
					{p.title}
					<span class="text-muted-foreground">· {p.body} · since {when(p.changed_at)}</span>
				</li>
			{/each}
		</ul>
	</div>
{/if}

<div class="mt-4 divide-y divide-border border-y border-border">
	<section class="py-5">
		<div class="flex flex-wrap items-start justify-between gap-3">
			<div class="max-w-2xl text-sm">
				<h2 class="font-semibold text-foreground">Web Push</h2>
				<p class="mt-1 text-muted-foreground">
					Phones and computers that turned notifications on, under <a
						href={resolve('/settings')}
						class="underline underline-offset-2 hover:text-foreground">This browser</a
					>.
					{#if view}{view.channels.push.devices === 0
							? 'None yet.'
							: `${view.channels.push.devices} device${view.channels.push.devices === 1 ? '' : 's'}.`}{/if}
				</p>
				{#if view}
					{@const s = statusLine(view.channels.push.status, when)}
					{#if s}<p
							class="mt-1 text-xs [overflow-wrap:anywhere] {s.ok
								? 'text-muted-foreground'
								: 'text-red-400'}"
						>
							{s.text}
						</p>{/if}
				{/if}
			</div>
			<button
				class={secondary}
				onclick={() => test('push')}
				disabled={busy['test-push'] || !view?.channels.push.devices}>Send a test</button
			>
		</div>
		{@render outcome('push')}
	</section>

	{#each CHANNELS as ch (ch.id)}
		{@const need = missing(ch, form)}
		{@const s = view ? statusLine(view.channels[ch.id].status, when) : null}
		<section class="py-5" aria-labelledby="ch-{ch.id}">
			<div class="flex flex-wrap items-start justify-between gap-3">
				<div class="max-w-2xl text-sm">
					<h2 id="ch-{ch.id}" class="font-semibold text-foreground">{ch.name}</h2>
					<p class="mt-1 text-muted-foreground">{ch.blurb}</p>
					{#if s}<p
							class="mt-1 text-xs [overflow-wrap:anywhere] {s.ok
								? 'text-muted-foreground'
								: 'text-red-400'}"
						>
							{s.text}
						</p>{/if}
				</div>
				<label class="flex items-center gap-2 text-sm text-foreground">
					<input type="checkbox" bind:checked={form.enabled[ch.id]} class="h-4 w-4" />
					On
				</label>
			</div>
			<div class="mt-3 grid gap-3 sm:grid-cols-2">
				{#each ch.fields as f (f.key)}
					<label
						class="block text-sm text-foreground {ch.fields.length === 1 ? 'sm:col-span-2' : ''}"
					>
						{f.label}
						<input
							class="{input} mt-1"
							type={f.secret ? 'password' : f.type === 'url' ? 'url' : 'text'}
							autocomplete="off"
							spellcheck="false"
							placeholder={f.secret && form.isSet[f.key]
								? 'Set; leave empty to keep it'
								: (f.placeholder ?? '')}
							bind:value={form.values[f.key]}
						/>
						{#if f.secret && form.isSet[f.key]}
							<span class="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
								Stored on the server; it is never shown.
								<button
									type="button"
									class="underline underline-offset-2 hover:text-foreground"
									onclick={() => forget(ch, f.key)}>Remove</button
								>
							</span>
						{/if}
					</label>
				{/each}
			</div>
			<div class="mt-3 flex flex-wrap items-center gap-2">
				<button class={primary} onclick={() => save(ch)} disabled={busy[ch.id]}>Save</button>
				<button
					class={secondary}
					onclick={() => test(ch.id, channelValues(ch, form))}
					disabled={busy[`test-${ch.id}`] || !!need}
				>
					{busy[`test-${ch.id}`] ? 'Sending…' : 'Test'}
				</button>
				{#if need}<span class="text-xs text-muted-foreground">{need}</span>{/if}
			</div>
			{@render outcome(ch.id)}
		</section>
	{/each}

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">System events</h2>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			Sent once when a problem starts and once when it clears, through the same channels. Alarms
			during a roast are set under <a
				href={resolve('/settings/alarms')}
				class="underline underline-offset-2 hover:text-foreground">Alarms</a
			>.
		</p>
		<div class="mt-3 space-y-2 text-sm text-foreground">
			<label class="flex items-center gap-2"
				><input type="checkbox" bind:checked={events.backup} class="h-4 w-4" /> The nightly backup failed</label
			>
			<label class="flex flex-wrap items-center gap-2">
				<input type="checkbox" bind:checked={events.offline} class="h-4 w-4" />
				A roaster sent nothing for
				<input
					type="number"
					min="1"
					max="1440"
					step="1"
					bind:value={events.offline_minutes}
					disabled={!events.offline}
					class={num}
					aria-label="Minutes"
				/>
				min, outside a roast
			</label>
			<label class="flex items-center gap-2"
				><input type="checkbox" bind:checked={events.stock} class="h-4 w-4" /> A bean fell below its low-stock
				mark</label
			>
		</div>
		<button class="{primary} mt-3" onclick={saveEvents}>Save</button>
		{@render outcome('events')}
	</section>
</div>
