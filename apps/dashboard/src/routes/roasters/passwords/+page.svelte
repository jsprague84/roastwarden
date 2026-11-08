<script lang="ts">
	/**
	 * MQTT passwords (docs/add-roaster.md, "MQTT passwords"; mqtt_passwords.rs): each roaster
	 * signs in to the built-in broker with its id and a password of its own. One added here
	 * counts at once, no restart; the firmware download fills it in. A password is shown once.
	 */
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import { ArrowLeft, Check, Copy, Loader2 } from '@lucide/svelte';
	import { mqttPasswords } from '#lib/api/mqttPasswords.js';
	import { notifications } from '#lib/stores/notifications.js';
	import {
		passwordProblem,
		roasterIdProblem,
		sourceNote,
		taken,
		type MqttPasswordGiven,
		type MqttPasswordList
	} from '#lib/roaster/mqttPasswords.js';

	let { data }: PageProps = $props();
	// The loader's list, until an add or remove reads it again
	let list = $derived<MqttPasswordList | null>(data.list);

	let id = $state('');
	let own = $state('');
	let showOwn = $state(false);
	let busy = $state(false);
	let given = $state<(MqttPasswordGiven & { what: 'added' | 'changed' }) | null>(null);
	let copied = $state(false);

	const idProblem = $derived(
		id.trim()
			? (roasterIdProblem(id) ?? (taken(list, id) ? 'It has a password already' : null))
			: null
	);
	const ownProblem = $derived(showOwn ? passwordProblem(own) : null);
	const msg = (e: unknown) =>
		e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);
	const when = (iso: string) =>
		new Date(iso).toLocaleDateString(undefined, { dateStyle: 'medium' });

	async function reload() {
		try {
			list = await mqttPasswords.list();
		} catch (e) {
			notifications.add(`Could not read the passwords: ${msg(e)}`, 'error');
		}
	}

	async function add(e: SubmitEvent) {
		e.preventDefault();
		if (!id.trim() || idProblem || ownProblem) return;
		busy = true;
		try {
			const r = await mqttPasswords.add(id.trim(), showOwn ? own : undefined);
			given = { ...r, what: 'added' };
			copied = false;
			id = '';
			own = '';
			showOwn = false;
			await reload();
		} catch (err) {
			notifications.add(`Could not add the password: ${msg(err)}`, 'error');
		} finally {
			busy = false;
		}
	}

	async function renew(roasterId: string, signedIn: boolean) {
		const warn = signedIn
			? `${roasterId} is connected now: a new password signs it out until its sketch has the new one. Go on?`
			: `A new password for ${roasterId}? Its sketch needs the new one.`;
		if (!confirm(warn)) return;
		try {
			const r = await mqttPasswords.change(roasterId);
			given = { ...r, what: 'changed' };
			copied = false;
			await reload();
		} catch (e) {
			notifications.add(`Could not change the password: ${msg(e)}`, 'error');
		}
	}

	async function remove(roasterId: string) {
		if (
			!confirm(
				`Remove ${roasterId}'s password? It is signed out and can no longer sign in. Its roasts and setup stay.`
			)
		)
			return;
		try {
			await mqttPasswords.remove(roasterId);
			if (given?.roaster_id === roasterId) given = null;
			await reload();
		} catch (e) {
			notifications.add(`Could not remove the password: ${msg(e)}`, 'error');
		}
	}

	async function copy() {
		if (!given) return;
		try {
			await navigator.clipboard.writeText(given.password);
			copied = true;
		} catch {
			notifications.add('Copy failed: select the password and copy it by hand.', 'error');
		}
	}

	const input =
		'mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus';
	const small = 'text-xs text-muted-foreground underline hover:text-foreground';
</script>

<svelte:head>
	<title>MQTT passwords | Roastwarden</title>
</svelte:head>

<div class="max-w-3xl space-y-5">
	<div>
		<a
			href={resolve('/roasters')}
			class="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
			><ArrowLeft class="h-3.5 w-3.5" /> Roasters</a
		>
		<h1 class="mt-1 text-2xl font-bold text-foreground">MQTT passwords</h1>
		<p class="mt-1 text-sm text-muted-foreground">
			A roaster that sends on MQTT signs in with its id and a password of its own. One added here
			works at once, with no restart, and <a
				href={resolve('/roasters/firmware')}
				class="underline hover:text-foreground">ESP32 firmware</a
			> fills it into the sketch.
		</p>
	</div>

	{#if !list}
		<p class="text-sm text-muted-foreground" data-testid="passwords-denied">
			{data.status === 401 || data.status === 403
				? 'Sign in with the owner key (or an admin token) to see who may sign in.'
				: `The server did not answer (${data.status}).`}
		</p>
	{:else}
		{#if !list.built_in_broker}
			<p
				class="border-l-2 border-highlight pl-3 text-sm text-foreground"
				data-testid="passwords-outside"
			>
				This server uses an MQTT broker of its own, so a roaster's user and password are added in
				that broker (for Mosquitto, its password file), not here:
				<a
					class="text-action underline"
					href="https://roastwarden.com/docs/install-docker/#with-your-own-mqtt-broker"
					target="_blank"
					rel="noopener">the users and topics it needs</a
				>.
			</p>
		{:else}
			<form class="space-y-3" onsubmit={add} data-testid="passwords-add">
				<div class="grid gap-4 sm:grid-cols-2">
					<div>
						<label for="pw-id" class="block text-sm font-medium text-foreground">Roaster id</label>
						<input
							id="pw-id"
							bind:value={id}
							placeholder="garage_roaster"
							class="{input} font-mono"
							autocomplete="off"
						/>
						<p class="mt-1 text-xs {idProblem ? 'text-destructive' : 'text-muted-foreground'}">
							{idProblem ??
								'As the roaster sends it: its MQTT user, and its topics roaster/<id>/….'}
						</p>
					</div>
					<div>
						{#if showOwn}
							<label for="pw-own" class="block text-sm font-medium text-foreground"
								>Its password</label
							>
							<input
								id="pw-own"
								type="password"
								bind:value={own}
								class={input}
								autocomplete="new-password"
							/>
							<p class="mt-1 text-xs {ownProblem ? 'text-destructive' : 'text-muted-foreground'}">
								{ownProblem ?? 'The one already in its sketch. '}
								<button type="button" class={small} onclick={() => ((showOwn = false), (own = ''))}
									>Make one instead</button
								>
							</p>
						{:else}
							<span class="block text-sm font-medium text-foreground">Its password</span>
							<p class="mt-2 text-sm text-muted-foreground">
								A random one, shown once.
								<button type="button" class={small} onclick={() => (showOwn = true)}
									>Use the sketch's own</button
								>
							</p>
						{/if}
					</div>
				</div>
				<button
					type="submit"
					disabled={busy || !id.trim() || !!idProblem || !!ownProblem || (showOwn && !own)}
					class="inline-flex items-center gap-1.5 rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>
					{#if busy}<Loader2 class="h-4 w-4 animate-spin" />{/if}
					Add
				</button>
			</form>
		{/if}

		{#if given}
			<div class="space-y-1 border-l-2 border-go pl-3 text-sm" data-testid="passwords-given">
				<p class="text-foreground">
					{given.roaster_id}'s {given.what === 'added' ? 'password' : 'new password'}:
					<code class="select-all rounded bg-background px-1 font-mono">{given.password}</code>
					<button
						type="button"
						onclick={copy}
						class="ml-1 inline-flex items-center gap-1 align-middle text-xs text-muted-foreground hover:text-foreground"
					>
						{#if copied}<Check class="h-3.5 w-3.5" /> Copied{:else}<Copy class="h-3.5 w-3.5" /> Copy{/if}
					</button>
				</p>
				<p class="text-muted-foreground">
					Shown once. Building it with an ESP32?
					<a
						href={resolve(`/roasters/firmware?id=${encodeURIComponent(given.roaster_id)}`)}
						class="underline hover:text-foreground">Download its firmware</a
					>
					with the password filled in. Otherwise put it in the roaster's sketch as its MQTT password.
				</p>
			</div>
		{/if}

		<section>
			<h2 class="text-sm font-semibold text-foreground">Who may sign in</h2>
			{#if list.passwords.length === 0}
				<p class="mt-2 text-sm text-muted-foreground">No roaster yet.</p>
			{:else}
				<ul class="mt-2 divide-y divide-border border-y border-border" data-testid="passwords-list">
					{#each list.passwords as p (p.roaster_id)}
						<li class="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 py-2.5 text-sm">
							<span class="min-w-0">
								<span class="font-mono text-foreground">{p.roaster_id}</span>
								{#if p.signed_in}<span class="ml-2 text-xs text-go">connected</span>{/if}
								<span class="block text-xs text-muted-foreground">
									{sourceNote(p, when)}{p.added ? '' : ' · not set up under Roasters yet'}
								</span>
							</span>
							{#if p.source === 'dashboard'}
								<span class="flex items-center gap-3">
									<button
										type="button"
										class={small}
										onclick={() => renew(p.roaster_id, p.signed_in)}>New password</button
									>
									<button
										type="button"
										class="text-xs text-destructive underline hover:opacity-80"
										onclick={() => remove(p.roaster_id)}>Remove</button
									>
								</span>
							{:else}
								<span class="text-xs text-muted-foreground">Changed in .env, with a restart</span>
							{/if}
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}
</div>
