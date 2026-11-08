<script lang="ts">
	/**
	 * Home Assistant (docs/home-assistant.md): each roaster as a Home Assistant device over MQTT
	 * discovery, read-only. Off until switched on; the server's own broker or Home Assistant's.
	 */
	import { homeAssistantApi, type HaStatus } from '#lib/api/homeAssistant.js';
	import { settings } from '#lib/api/settings.js';
	import { fromStatus, problem, statusLine, values, type HaForm } from '#lib/homeAssistant/form.js';

	let view = $state<HaStatus | null>(null);
	let form = $state<HaForm | null>(null);
	let busy = $state(false);
	let result = $state<{ ok: boolean; text: string } | null>(null);

	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);

	async function load(withForm: boolean) {
		try {
			const [s, stored] = await Promise.all([
				homeAssistantApi.status(),
				withForm ? settings.get() : Promise.resolve(null)
			]);
			view = s;
			if (stored) form = fromStatus(s, stored);
		} catch (e) {
			result = { ok: false, text: `Could not read the Home Assistant settings: ${msg(e)}` };
		}
	}

	$effect(() => {
		void load(true);
		// The status follows the server's publishing; the form stays as typed
		const timer = setInterval(() => void load(false), 5000);
		return () => clearInterval(timer);
	});

	async function save() {
		if (!form) return;
		busy = true;
		try {
			await settings.setMany(values(form));
			// The server reads its settings every second
			await new Promise((r) => setTimeout(r, 1500));
			await load(true);
			result = {
				ok: true,
				text: form?.enabled ? 'Saved: on.' : 'Saved: off. Home Assistant removes the roasters.'
			};
		} catch (e) {
			result = { ok: false, text: msg(e) };
		} finally {
			busy = false;
		}
	}

	async function forgetPassword() {
		try {
			await settings.set('ha_password', '');
			await load(false);
			result = { ok: true, text: 'Password removed.' };
		} catch (e) {
			result = { ok: false, text: msg(e) };
		}
	}

	const need = $derived(form ? problem(form) : null);
	const line = $derived(view ? statusLine(view) : null);
	const when = (iso: string) =>
		new Date(iso).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'medium' });
	const input =
		'w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus disabled:opacity-50';
	const num =
		'w-20 rounded-md border border-border bg-input px-2 py-1 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const primary =
		'rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50';
</script>

<p class="max-w-2xl text-sm text-muted-foreground">
	Each roaster shows up in Home Assistant as a device: bean and environment temperature, rate of
	rise, heater, fan, setpoint, the roast's phase and time, and the RoR warning. Read only: Home
	Assistant cannot send a roaster anything.
</p>

{#if form}
	<div class="mt-4 divide-y divide-border border-y border-border">
		<section class="py-5" aria-labelledby="ha-on">
			<div class="flex flex-wrap items-start justify-between gap-3">
				<div class="max-w-2xl text-sm">
					<h2 id="ha-on" class="font-semibold text-foreground">Discovery</h2>
					{#if line}<p
							class="mt-1 text-xs {line.ok ? 'text-muted-foreground' : 'text-red-400'}"
							role="status"
						>
							{line.text}
						</p>{/if}
				</div>
				<label class="flex items-center gap-2 text-sm text-foreground">
					<input type="checkbox" bind:checked={form.enabled} class="h-4 w-4" />
					On
				</label>
			</div>
		</section>

		<section class="py-5" aria-labelledby="ha-broker">
			<h2 id="ha-broker" class="text-sm font-semibold text-foreground">Broker</h2>
			<div class="mt-2 space-y-2 text-sm text-foreground">
				<label class="flex items-start gap-2">
					<input type="radio" bind:group={form.where} value="server" class="mt-0.5" />
					<span>
						The server's broker, the one the roasters use
						{#if view?.built_in_user}
							<span class="block text-xs text-muted-foreground">
								Home Assistant signs in to it as <code>{view.built_in_user}</code> with the password
								in <code>MQTT_HOME_ASSISTANT_PASSWORD</code>.
							</span>
						{:else if view?.built_in_without_user}
							<span class="block text-xs text-amber-400">
								The built-in broker has no Home Assistant user yet: set <code
									>MQTT_HOME_ASSISTANT_PASSWORD</code
								> in .env and restart.
							</span>
						{:else}
							<span class="block text-xs text-muted-foreground">
								Home Assistant needs a user there that may read <code>homeassistant/#</code> and
								<code>roastwarden/ha/#</code>
								(the compose file's Mosquitto has one: <code>MQTT_HOME_ASSISTANT_PASSWORD</code>).
							</span>
						{/if}
					</span>
				</label>
				<label class="flex items-start gap-2">
					<input type="radio" bind:group={form.where} value="other" class="mt-0.5" />
					<span>Home Assistant's own broker</span>
				</label>
			</div>
			{#if form.where === 'other'}
				<div class="mt-3 grid gap-3 sm:grid-cols-3">
					<label class="block text-sm text-foreground">
						Address
						<input
							class="{input} mt-1"
							type="text"
							autocomplete="off"
							spellcheck="false"
							placeholder="mqtt://homeassistant.local:1883"
							bind:value={form.broker}
						/>
					</label>
					<label class="block text-sm text-foreground">
						User
						<input
							class="{input} mt-1"
							type="text"
							autocomplete="off"
							spellcheck="false"
							bind:value={form.username}
						/>
					</label>
					<label class="block text-sm text-foreground">
						Password
						<input
							class="{input} mt-1"
							type="password"
							autocomplete="new-password"
							placeholder={view?.password_set ? 'Set; leave empty to keep it' : ''}
							bind:value={form.password}
						/>
						{#if view?.password_set}
							<span class="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
								Stored on the server; it is never shown.
								<button
									type="button"
									class="underline underline-offset-2 hover:text-foreground"
									onclick={forgetPassword}>Remove</button
								>
							</span>
						{/if}
					</label>
				</div>
			{/if}
		</section>

		<section class="py-5" aria-labelledby="ha-more">
			<h2 id="ha-more" class="text-sm font-semibold text-foreground">Options</h2>
			<div class="mt-3 flex flex-wrap items-end gap-x-6 gap-y-3 text-sm text-foreground">
				<label class="block">
					<span
						>Discovery prefix <span class="text-xs text-muted-foreground"
							>(Home Assistant's: homeassistant unless changed there)</span
						></span
					>
					<input
						class="{input} mt-1 !w-48"
						type="text"
						spellcheck="false"
						bind:value={form.prefix}
					/>
				</label>
				<label class="flex items-center gap-2">
					Every
					<input
						type="number"
						min="1"
						max="60"
						step="1"
						bind:value={form.interval}
						class={num}
						aria-label="Seconds between updates"
					/>
					s
				</label>
				<label class="flex items-center gap-2">
					<input type="checkbox" bind:checked={form.includeVirtual} class="h-4 w-4" />
					The simulated roasters too
				</label>
			</div>
		</section>
	</div>

	<div class="mt-4 flex flex-wrap items-center gap-2">
		<button class={primary} onclick={save} disabled={busy || !!need}
			>{busy ? 'Saving…' : 'Save'}</button
		>
		{#if need}<span class="text-xs text-muted-foreground">{need}</span>{/if}
	</div>
{/if}
{#if result}<p class="mt-2 text-sm {result.ok ? 'text-green-400' : 'text-red-400'}" role="status">
		{result.text}
	</p>{/if}

{#if view?.enabled && view.roasters.length}
	<section class="mt-6" aria-labelledby="ha-roasters">
		<h2 id="ha-roasters" class="text-sm font-semibold text-foreground">In Home Assistant</h2>
		<ul class="mt-2 divide-y divide-border border-y border-border text-sm">
			{#each view.roasters as r (r.device_id)}
				<li class="py-2">
					<span class="font-medium text-foreground">{r.name}</span>
					<span class="text-muted-foreground"
						>· {r.available ? 'available' : 'unavailable (not reporting)'}</span
					>
					<span class="block text-xs text-muted-foreground [overflow-wrap:anywhere]"
						>{r.entities.length} entities: {r.entities.join(', ')}</span
					>
				</li>
			{/each}
		</ul>
		{#if view.last_published_at}<p class="mt-1 text-xs text-muted-foreground">
				Last update {when(view.last_published_at)}.
			</p>{/if}
	</section>
{/if}
