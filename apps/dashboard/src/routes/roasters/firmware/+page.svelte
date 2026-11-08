<script lang="ts">
	/**
	 * ESP32 firmware (docs/esp32-starter.md, docs/esp32-fluid-bed.md): the program for a roaster
	 * someone builds, with this server's settings filled in, as a PlatformIO project in a zip that
	 * the Arduino IDE opens too. Nothing typed here is stored.
	 */
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import { ArrowLeft, Download, Loader2 } from '@lucide/svelte';
	import { page } from '$app/state';
	import { firmware } from '#lib/api/firmware.js';
	import { mqttPasswords } from '#lib/api/mqttPasswords.js';
	import {
		defaultAddress,
		firmwareRequest,
		formProblem,
		idProblem,
		missingNote,
		type FirmwareForm,
		type FirmwareKind
	} from '#lib/roaster/firmware.js';

	let { data }: PageProps = $props();
	const info = $derived(data.info);

	const input =
		'mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus';

	let form = $state<FirmwareForm>({
		kind: 'starter',
		roasterId: 'my_roaster',
		password: '',
		address: defaultAddress(typeof location === 'undefined' ? '' : location.hostname),
		port: null,
		wifiName: '',
		wifiPassword: '',
		board: 'max31855',
		etProbe: true
	});

	// An id with a password on the server: it fills the password in. Start from ?id=, else the
	// first one not added.
	$effect.pre(() => {
		const asked = page.url.searchParams.get('id');
		const first = info?.roasters.find((r) => !r.added) ?? info?.roasters[0];
		if (asked) form.roasterId = asked;
		else if (first) form.roasterId = first.id;
	});
	// Passwords made on this page (Make its password) count as the server's
	let made = $state<string[]>([]);
	const known = $derived(
		(info?.roasters.some((r) => r.id === form.roasterId.trim()) ?? false) ||
			made.includes(form.roasterId.trim())
	);

	let making = $state(false);
	let makeError = $state('');
	async function makePassword() {
		const id = form.roasterId.trim();
		making = true;
		makeError = '';
		try {
			await mqttPasswords.add(id);
			made = [...made, id];
			form.password = '';
		} catch (e) {
			makeError = e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);
		} finally {
			making = false;
		}
	}
	const problem = $derived(formProblem(form));

	let busy = $state(false);
	let error = $state('');
	let done = $state<{ file: string; missing: string[]; id: string; kind: FirmwareKind } | null>(
		null
	);

	async function download() {
		busy = true;
		error = '';
		try {
			const r = await firmware.download(firmwareRequest(form));
			done = { ...r, id: form.roasterId.trim(), kind: form.kind };
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	const guideFor = (k: FirmwareKind) =>
		info?.kinds.find((x) => x.kind === k)?.guide ?? 'https://roastwarden.com/docs/esp32-starter/';
</script>

<svelte:head>
	<title>ESP32 firmware | Roastwarden</title>
</svelte:head>

<div class="max-w-3xl space-y-5">
	<div>
		<a
			href={resolve('/roasters')}
			class="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
			><ArrowLeft class="h-3.5 w-3.5" /> Roasters</a
		>
		<h1 class="mt-1 text-2xl font-bold text-foreground">ESP32 firmware</h1>
		<p class="mt-1 text-sm text-muted-foreground">
			For a roaster you build: the program for its ESP32 with this server's settings filled in.
			Upload it with VS Code and PlatformIO, or the Arduino IDE; the roaster then shows up under
			Roasters, ready to set up.
		</p>
	</div>

	{#if !info}
		<p class="text-sm text-muted-foreground" data-testid="firmware-denied">
			{data.status === 401 || data.status === 403
				? 'Sign in with the owner key (or an admin token): the firmware carries the roaster’s password.'
				: `The server did not answer (${data.status}).`}
		</p>
	{:else}
		<fieldset>
			<legend class="text-sm font-medium text-foreground">What it does</legend>
			<div class="mt-2 grid gap-2 sm:grid-cols-2" role="radiogroup">
				{#each info.kinds as k (k.kind)}
					<label
						class="cursor-pointer rounded-md border p-3 text-sm {form.kind === k.kind
							? 'border-action'
							: 'border-border hover:border-muted-foreground'}"
						data-testid="firmware-kind-{k.kind}"
					>
						<input
							type="radio"
							name="firmware-kind"
							value={k.kind}
							bind:group={form.kind}
							class="sr-only"
						/>
						<span class="block font-medium text-foreground">{k.name}</span>
						<span class="mt-0.5 block text-xs text-muted-foreground">{k.summary}</span>
						<a
							href={k.guide}
							target="_blank"
							rel="external noopener"
							class="mt-1 inline-block text-xs underline hover:text-foreground">The guide</a
						>
					</label>
				{/each}
			</div>
		</fieldset>

		<div class="grid gap-4 sm:grid-cols-2">
			<div>
				<label for="fw-id" class="block text-sm font-medium text-foreground">Roaster id</label>
				<input
					id="fw-id"
					list="fw-ids"
					bind:value={form.roasterId}
					class="{input} font-mono"
					autocomplete="off"
				/>
				<datalist id="fw-ids">
					{#each info.roasters as r (r.id)}<option value={r.id}
							>{r.added ? 'added' : 'not added yet'}</option
						>{/each}
				</datalist>
				<p class="mt-1 text-xs text-muted-foreground" data-testid="firmware-password-note">
					{#if known}
						Its MQTT password is filled in (<a
							href={resolve('/roasters/passwords')}
							class="underline hover:text-foreground">MQTT passwords</a
						>).
					{:else if info.built_in_broker}
						No MQTT password yet: make one, or give the one it has below.
					{:else}
						Give its MQTT password below (the one your broker has for it), or fill it in later.
					{/if}
				</p>
			</div>
			{#if !known}
				<div>
					<label for="fw-password" class="block text-sm font-medium text-foreground"
						>Its MQTT password</label
					>
					{#if info.built_in_broker && !form.password}
						<button
							type="button"
							onclick={makePassword}
							disabled={making || idProblem(form.roasterId) !== null}
							class="mt-1 inline-flex items-center gap-1.5 rounded-md border border-border px-3 py-2 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-50"
							data-testid="firmware-make-password"
						>
							{#if making}<Loader2 class="h-4 w-4 animate-spin" />{/if}
							Make its password
						</button>
					{/if}
					<input
						id="fw-password"
						type="password"
						bind:value={form.password}
						placeholder={info.built_in_broker ? 'or the one it has' : ''}
						class={input}
						autocomplete="off"
					/>
					<p class="mt-1 text-xs {makeError ? 'text-destructive' : 'text-muted-foreground'}">
						{#if makeError}
							{makeError}
						{:else if info.built_in_broker && !form.password}
							A random one: the roaster may sign in with it at once, and the download fills it in.
						{:else if info.built_in_broker}
							Add it under <a
								href={resolve('/roasters/passwords')}
								class="underline hover:text-foreground">MQTT passwords</a
							> too, or the roaster cannot sign in.
						{:else}
							Left empty, you fill it in later.
						{/if}
					</p>
				</div>
			{/if}
		</div>
		<div class="grid gap-4 sm:grid-cols-2">
			<div>
				<label for="fw-address" class="block text-sm font-medium text-foreground"
					>This server's address</label
				>
				<input
					id="fw-address"
					bind:value={form.address}
					placeholder="192.168.1.50"
					class={input}
					autocomplete="off"
				/>
				<p class="mt-1 text-xs text-muted-foreground">
					As the ESP32 reaches it on your network (<code>hostname -I</code> on the server prints it).
				</p>
			</div>
			<div>
				<label for="fw-port" class="block text-sm font-medium text-foreground">MQTT port</label>
				<input
					id="fw-port"
					type="number"
					min="1"
					max="65535"
					bind:value={form.port}
					placeholder={String(info.port)}
					class={input}
				/>
				<p class="mt-1 text-xs text-muted-foreground">
					{info.built_in_broker
						? 'The built-in broker’s.'
						: 'Your broker’s; 1883 unless you moved it.'}
				</p>
			</div>
		</div>
		<div class="grid gap-4 sm:grid-cols-2">
			<div>
				<label for="fw-wifi" class="block text-sm font-medium text-foreground">Wi-Fi name</label>
				<input id="fw-wifi" bind:value={form.wifiName} class={input} autocomplete="off" />
				<p class="mt-1 text-xs text-muted-foreground">
					2.4 GHz: an ESP32 cannot join 5 GHz. Left empty, you fill it in later.
				</p>
			</div>
			<div>
				<label for="fw-wifi-password" class="block text-sm font-medium text-foreground"
					>Wi-Fi password</label
				>
				<input
					id="fw-wifi-password"
					type="password"
					bind:value={form.wifiPassword}
					class={input}
					autocomplete="off"
				/>
				<p class="mt-1 text-xs text-muted-foreground">
					Written into the file you download, never kept on the server.
				</p>
			</div>
		</div>

		<div class="flex flex-wrap items-center gap-x-6 gap-y-2 text-sm">
			<span class="flex items-center gap-2">
				<span class="font-medium text-foreground">Thermocouple board</span>
				{#each [['max31855', 'MAX31855'], ['max6675', 'MAX6675']] as [v, label] (v)}
					<label class="flex items-center gap-1 text-muted-foreground"
						><input type="radio" name="fw-board" value={v} bind:group={form.board} /> {label}</label
					>
				{/each}
			</span>
			<label class="flex items-center gap-2 text-muted-foreground">
				<input type="checkbox" bind:checked={form.etProbe} /> A second probe for ET
			</label>
		</div>

		{#if form.kind === 'fluid_bed'}
			<p class="text-xs text-muted-foreground">
				The heater and fan pins, the fan's kind (PWM or an ESC) and the safety limits are at the top
				of the sketch, with the wiring in <a
					href={guideFor('fluid_bed')}
					target="_blank"
					rel="external noopener"
					class="underline hover:text-foreground">the guide</a
				>. Run it the first time with the heating element disconnected.
			</p>
		{/if}

		<div class="flex flex-wrap items-center gap-3">
			<button
				type="button"
				onclick={download}
				disabled={busy || problem !== null}
				class="inline-flex items-center gap-1.5 rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				data-testid="firmware-download"
			>
				{#if busy}<Loader2 class="h-4 w-4 animate-spin" />{:else}<Download class="h-4 w-4" />{/if}
				Download
			</button>
			{#if problem}<span class="text-xs text-muted-foreground">{problem}</span>{/if}
			{#if error}<span class="text-xs text-destructive">{error}</span>{/if}
		</div>

		{#if done}
			<div class="space-y-1 border-l-2 border-go pl-3 text-sm" data-testid="firmware-done">
				<p class="text-foreground">
					Saved <span class="font-mono">{done.file}</span>. {missingNote(done.missing, done.id)}
				</p>
				<p class="text-muted-foreground">
					Unzip it, open the folder in VS Code with PlatformIO (or the sketch in the Arduino IDE)
					and upload it: the steps are in its README.txt and <a
						href={guideFor(done.kind)}
						target="_blank"
						rel="external noopener"
						class="underline hover:text-foreground">the guide</a
					>. Then <span class="font-mono">{done.id}</span> shows up under
					<a href={resolve('/roasters')} class="underline hover:text-foreground">Roasters</a>,
					“Found, not added yet”: Set up.
				</p>
			</div>
		{/if}
	{/if}
</div>
