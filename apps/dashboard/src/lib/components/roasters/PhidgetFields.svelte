<script lang="ts">
	/**
	 * A Phidgets temperature board's fields (1048, TMP1101, …, read through Phidgets' Phidget22
	 * network server), for the connection form: `main` the host, the BT and ET inputs and the
	 * thermocouple type; `more` (the Advanced fold) the server's port and password, which board,
	 * the intervals and an RTD's element. The settings and their config are
	 * `lib/roaster/phidget.ts`; more inputs, a scale or an offset are the connection's Mapping.
	 */
	import Hint from '#lib/components/ui/Hint.svelte';
	import {
		INPUT_HELP,
		PHIDGET_INPUTS,
		RTD_LABEL,
		RTD_TYPES,
		THERMOCOUPLE_TYPES,
		phidgetProblems,
		type PhidgetSettings
	} from '#lib/roaster/phidget.js';

	let {
		settings = $bindable(),
		idPrefix = 'phidget',
		part = 'all',
		needsHost = false
	}: {
		settings: PhidgetSettings;
		idPrefix?: string;
		part?: 'all' | 'main' | 'more';
		/** A plan that cannot know the host: ask for it by name */
		needsHost?: boolean;
	} = $props();
	const main = $derived(part !== 'more');
	const more = $derived(part !== 'main');

	const input =
		'mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus';
	const label = 'block text-sm font-medium text-foreground';

	const hostEmpty = $derived(settings.host.trim() === '');
	// An empty host is plain from the form itself; the rest is listed
	const problems = $derived(
		phidgetProblems(settings).filter((p) => !(hostEmpty && p.startsWith('Give the computer')))
	);
	/** The saved mapping's other fields, in a line: `exhaust ch2`. */
	const extras = $derived(
		Object.entries(settings.fields)
			.filter(([k]) => k !== 'bt' && k !== 'et')
			.map(([k, v]) => `${k} ${typeof v === 'string' ? v : ((v as { path?: string })?.path ?? '')}`)
	);
	const ownTypes = $derived(
		Object.entries(settings.thermocoupleTypes ?? {}).map(([k, t]) => `${k} ${t}`)
	);
</script>

<div class="space-y-4">
	{#if main}
		<div>
			<label for="{idPrefix}-host" class={label}>Computer it is plugged into</label>
			<input
				id="{idPrefix}-host"
				type="text"
				bind:value={settings.host}
				placeholder="phidgetsbc.local or 192.168.1.20"
				class={input}
			/>
			<p class="mt-1 text-xs {needsHost && hostEmpty ? 'text-highlight' : 'text-muted-foreground'}">
				{#if needsHost && hostEmpty}Give its address; Artisan's file cannot know it.{/if}
				The computer the board's USB goes into, running Phidgets' network server (Phidget Control Panel
				→ Network Server).
			</p>
		</div>
		<div class="grid grid-cols-3 gap-4">
			<div>
				<span class="flex items-center gap-1 text-sm font-medium text-foreground">
					<label for="{idPrefix}-bt">BT input</label>
					<Hint inline help={INPUT_HELP} />
				</span>
				<select id="{idPrefix}-bt" bind:value={settings.bt} class={input}>
					{#each PHIDGET_INPUTS as c (c)}<option value={c}>{c}</option>{/each}
				</select>
			</div>
			<div>
				<label for="{idPrefix}-et" class={label}>ET input</label>
				<select id="{idPrefix}-et" bind:value={settings.et} class={input}>
					{#each PHIDGET_INPUTS as c (c)}<option value={c}>{c}</option>{/each}
					<option value="">none</option>
				</select>
			</div>
			<div>
				<label for="{idPrefix}-tc" class={label}>Thermocouple</label>
				<select id="{idPrefix}-tc" bind:value={settings.thermocoupleType} class={input}>
					{#each THERMOCOUPLE_TYPES as t (t)}<option value={t}>Type {t}</option>{/each}
				</select>
			</div>
		</div>
		{#if problems.length > 0}
			<ul class="space-y-0.5 text-xs text-highlight">
				{#each problems as p (p)}<li>{p}</li>{/each}
			</ul>
		{/if}
	{/if}
	{#if more}
		<div class="grid grid-cols-[6rem_minmax(0,1fr)] gap-4">
			<div>
				<label for="{idPrefix}-port" class={label}>Port</label>
				<input
					id="{idPrefix}-port"
					type="number"
					bind:value={settings.port}
					min={1}
					max={65535}
					class={input}
				/>
			</div>
			<div>
				<label for="{idPrefix}-password" class={label}>Network server password</label>
				<div class="flex items-center gap-2">
					<input
						id="{idPrefix}-password"
						type="password"
						autocomplete="new-password"
						bind:value={settings.password}
						placeholder={settings.passwordSaved ? 'saved: type a new one to change it' : 'none'}
						class={input}
					/>
					{#if settings.passwordSaved}
						<button
							type="button"
							onclick={() => {
								settings.passwordSaved = false;
								settings.password = '';
							}}
							class="mt-1 shrink-0 text-xs text-muted-foreground underline hover:text-foreground"
							>Forget it</button
						>
					{/if}
				</div>
			</div>
		</div>
		<div class="grid grid-cols-2 gap-4">
			<div>
				<label for="{idPrefix}-serial" class={label}>Board serial number</label>
				<input
					id="{idPrefix}-serial"
					type="number"
					bind:value={settings.serialNumber}
					min={1}
					placeholder="the first board"
					class={input}
				/>
			</div>
			<div>
				<label for="{idPrefix}-hub-port" class={label}>VINT hub port</label>
				<select id="{idPrefix}-hub-port" bind:value={settings.hubPort} class={input}>
					<option value={null}>any</option>
					{#each [0, 1, 2, 3, 4, 5] as p (p)}<option value={p}>{p}</option>{/each}
				</select>
			</div>
		</div>
		<p class="-mt-2 text-xs text-muted-foreground">
			Only with more than one board on the server; a VINT board (TMP1101) goes by its hub's serial
			number.
		</p>
		<div class="grid grid-cols-2 gap-4">
			<div>
				<label for="{idPrefix}-data" class={label}>Board reports every (ms)</label>
				<input
					id="{idPrefix}-data"
					type="number"
					bind:value={settings.dataIntervalMs}
					min={8}
					max={60000}
					step={8}
					class={input}
				/>
			</div>
			<div>
				<label for="{idPrefix}-poll" class={label}>Sample every (ms)</label>
				<input
					id="{idPrefix}-poll"
					type="number"
					bind:value={settings.pollIntervalMs}
					min={100}
					step={100}
					class={input}
				/>
			</div>
		</div>
		<p class="-mt-2 text-xs text-muted-foreground">
			A sample is the mean of the readings since the last one.
		</p>
		<div class="grid grid-cols-2 gap-4">
			<div>
				<label for="{idPrefix}-rtd" class={label}>RTD element</label>
				<select id="{idPrefix}-rtd" bind:value={settings.rtdType} class={input}>
					{#each RTD_TYPES as t (t)}<option value={t}>{RTD_LABEL[t]}</option>{/each}
				</select>
			</div>
			<div>
				<label for="{idPrefix}-rtd-wires" class={label}>RTD wires</label>
				<select id="{idPrefix}-rtd-wires" bind:value={settings.rtdWires} class={input}>
					{#each [2, 3, 4] as w (w)}<option value={w}>{w}</option>{/each}
				</select>
			</div>
		</div>
		<p class="-mt-2 text-xs text-muted-foreground">An RTD board (TMP1200) only.</p>
		<p class="text-xs text-muted-foreground">
			{#if extras.length || ownTypes.length}
				Kept from its mapping: {[...extras, ...ownTypes.map((t) => `type ${t}`)].join(', ')}.
			{/if}
			More inputs as channels (exhaust, inlet), a scale or an offset: the connection's Mapping, once it
			is saved.
		</p>
	{/if}
</div>
