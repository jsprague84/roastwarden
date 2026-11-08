<script lang="ts">
	/**
	 * One connection form for the Add roaster wizard and a roaster's Add connection: how
	 * the roaster connects, in plain words, the fields every roaster of that kind needs, and an
	 * Advanced fold for the rest (open stays open in this browser). The draft and its config are
	 * `lib/roaster/connectionForm.ts`.
	 */
	import Hint from '#lib/components/ui/Hint.svelte';
	import Tc4Fields from '#lib/components/roasters/Tc4Fields.svelte';
	import PhidgetFields from '#lib/components/roasters/PhidgetFields.svelte';
	import { ASCII_HELP, RTU_BAUDS } from '#lib/roaster/connection.js';
	import {
		CONNECT_GUIDE,
		connectGuideUrl,
		mqttPrefixFor,
		PROTOCOL_CHOICES,
		type ConnectionDraft
	} from '#lib/roaster/connectionForm.js';
	import { placeholderHost } from '#lib/roaster/artisan.js';
	import { boardLine } from '#lib/roaster/phidget.js';
	import { about } from '#lib/api/about.js';

	let {
		draft = $bindable(),
		idPrefix = 'conn',
		deviceId = '',
		needs = []
	}: {
		draft: ConnectionDraft;
		idPrefix?: string;
		/** The roaster's id: the MQTT prefix's placeholder */
		deviceId?: string;
		/** What a plan cannot know: `host`, `port`, `telemetry_topic` */
		needs?: string[];
	} = $props();

	const input =
		'mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus';
	const label = 'block text-sm font-medium text-foreground';

	const FOLD_KEY = 'roastwarden.fold.connection-advanced';
	let advancedOpen = $state(false);
	try {
		advancedOpen = localStorage.getItem(FOLD_KEY) === '1';
	} catch {
		/* storage unavailable */
	}
	function onFold(e: Event) {
		advancedOpen = (e.currentTarget as HTMLDetailsElement).open;
		try {
			localStorage.setItem(FOLD_KEY, advancedOpen ? '1' : '0');
		} catch {
			/* storage unavailable */
		}
	}

	// The guide for a roaster you program yourself, in the source this server runs
	let guideUrl = $state<string | null>(null);
	about
		.get()
		.then((a) => (guideUrl = connectGuideUrl(a.source_url)))
		.catch(() => {});

	const needsTopic = $derived(draft.protocol === 'mqtt' && needs.includes('telemetry_topic'));
	const wsHostMissing = $derived(
		draft.protocol === 'websocket' && needs.includes('host') && placeholderHost(draft.ws.url)
	);

	/** What the fold holds now, so a closed fold still says it. */
	const advancedLine = $derived.by(() => {
		const d = draft;
		switch (d.protocol) {
			case 'mqtt':
				return `QoS ${d.mqtt.qos}${!needsTopic && d.mqtt.telemetryTopic.trim() ? ` · data on ${d.mqtt.telemetryTopic.trim()}` : ''}`;
			case 'websocket':
				return `reconnect ${d.ws.reconnectMs / 1000} s`;
			case 'modbus_tcp':
				return `unit ${d.tcp.unitId} · poll ${d.tcp.pollIntervalMs} ms`;
			case 'modbus_rtu':
				return `${d.rtu.framing === 'ascii' ? `ASCII, ${d.rtu.dataBits} data bits` : 'RTU'} · ${d.rtu.stopBits} stop bit${d.rtu.stopBits > 1 ? 's' : ''} · poll ${d.rtu.pollIntervalMs} ms`;
			case 'serial_tc4':
				return `${d.tc4.baud} baud · ET ${d.tc4.et_channel}, BT ${d.tc4.bt_channel} · fan ${d.tc4.fan_output}`;
			case 'phidget22': {
				const p = d.phidget;
				const pw = p.password !== '' || p.passwordSaved ? ' · password' : '';
				const board = boardLine(p) || 'the first board';
				return `port ${p.port}${pw} · ${board} · reports every ${p.dataIntervalMs} ms`;
			}
		}
		return '';
	});
</script>

<div class="space-y-5">
	<fieldset>
		<legend class="text-sm font-medium text-foreground">How does it connect?</legend>
		<div class="mt-2 divide-y divide-border rounded-md border border-border" role="radiogroup">
			{#each PROTOCOL_CHOICES as c (c.protocol)}
				<label
					class="flex cursor-pointer items-start gap-3 px-3 py-2 hover:bg-accent {draft.protocol ===
					c.protocol
						? 'bg-accent/60'
						: ''}"
					data-testid="protocol-{c.protocol}"
				>
					<input
						type="radio"
						name="{idPrefix}-protocol"
						value={c.protocol}
						bind:group={draft.protocol}
						class="mt-1"
					/>
					<span class="min-w-0">
						<span class="block text-sm font-medium text-foreground">{c.title}</span>
						<span class="block text-xs text-muted-foreground">{c.line}</span>
					</span>
				</label>
			{/each}
		</div>
		<p class="mt-1.5 text-xs text-muted-foreground" data-testid="connect-guide">
			Programming your own roaster? {#if guideUrl}<a
					href={guideUrl}
					target="_blank"
					rel="external noopener"
					class="underline hover:text-foreground">Connect your roaster</a
				>{:else}<code>{CONNECT_GUIDE}</code>{/if} says what Roastwarden expects, with a working example
			for each way in.
		</p>
	</fieldset>

	<div class="space-y-4">
		{#if draft.protocol === 'mqtt'}
			<div>
				<label for="{idPrefix}-mqtt-prefix" class={label}>Topic prefix</label>
				<input
					id="{idPrefix}-mqtt-prefix"
					type="text"
					bind:value={draft.mqtt.topicPrefix}
					placeholder={mqttPrefixFor(deviceId || 'my_roaster')}
					class={input}
				/>
				<p class="mt-1 text-xs text-muted-foreground">
					Its readings arrive on <code
						>{draft.mqtt.topicPrefix.replace(/\/+$/, '') || 'roaster/…'}/telemetry</code
					>; commands go under <code>…/control/</code>. Roastwarden's firmware uses
					<code>roaster/</code> and the roaster's id.
				</p>
			</div>
			{#if needsTopic}
				<div>
					<label for="{idPrefix}-mqtt-telemetry" class={label}
						>Topic its data arrives on <span class="text-red-400">*</span></label
					>
					<input
						id="{idPrefix}-mqtt-telemetry"
						type="text"
						bind:value={draft.mqtt.telemetryTopic}
						class={input}
					/>
				</div>
			{/if}
		{:else if draft.protocol === 'websocket'}
			<div>
				<label for="{idPrefix}-ws-url" class={label}>WebSocket address</label>
				<input
					id="{idPrefix}-ws-url"
					type="text"
					bind:value={draft.ws.url}
					placeholder="ws://192.168.1.100:8080/ws"
					class={input}
				/>
				{#if wsHostMissing}
					<p class="mt-1 text-xs text-highlight">
						Replace 127.0.0.1 with the roaster's address; the path and port are Artisan's.
					</p>
				{/if}
			</div>
		{:else if draft.protocol === 'modbus_tcp'}
			<div class="grid grid-cols-[minmax(0,1fr)_6rem] gap-4">
				<div>
					<label for="{idPrefix}-tcp-host" class={label}>Address</label>
					<input
						id="{idPrefix}-tcp-host"
						type="text"
						bind:value={draft.tcp.host}
						placeholder="192.168.1.100"
						class={input}
					/>
				</div>
				<div>
					<label for="{idPrefix}-tcp-port" class={label}>Port</label>
					<input
						id="{idPrefix}-tcp-port"
						type="number"
						bind:value={draft.tcp.port}
						min={1}
						max={65535}
						class={input}
					/>
				</div>
			</div>
		{:else if draft.protocol === 'modbus_rtu'}
			<div>
				<label for="{idPrefix}-rtu-port" class={label}>Serial port</label>
				<input
					id="{idPrefix}-rtu-port"
					type="text"
					bind:value={draft.rtu.port}
					placeholder="/dev/ttyUSB0"
					class={input}
				/>
				<p class="mt-1 text-xs text-muted-foreground">
					An RS-485 (or RS-232) adapter on the server. Baud, parity and unit ID must match the
					controller's own settings.
				</p>
			</div>
			<div class="grid grid-cols-3 gap-4">
				<div>
					<label for="{idPrefix}-rtu-baud" class={label}>Baud</label>
					<select id="{idPrefix}-rtu-baud" bind:value={draft.rtu.baud} class={input}>
						{#each RTU_BAUDS as b (b)}<option value={b}>{b}</option>{/each}
					</select>
				</div>
				<div>
					<label for="{idPrefix}-rtu-parity" class={label}>Parity</label>
					<select id="{idPrefix}-rtu-parity" bind:value={draft.rtu.parity} class={input}>
						<option value="none">None</option>
						<option value="even">Even</option>
						<option value="odd">Odd</option>
					</select>
				</div>
				<div>
					<label for="{idPrefix}-rtu-unit" class={label}>Unit ID</label>
					<input
						id="{idPrefix}-rtu-unit"
						type="number"
						bind:value={draft.rtu.unitId}
						min={1}
						max={247}
						class={input}
					/>
				</div>
			</div>
		{:else if draft.protocol === 'serial_tc4'}
			<Tc4Fields bind:settings={draft.tc4} idPrefix="{idPrefix}-tc4" part="main" />
		{:else if draft.protocol === 'phidget22'}
			<PhidgetFields
				bind:settings={draft.phidget}
				idPrefix="{idPrefix}-phidget"
				part="main"
				needsHost={needs.includes('host')}
			/>
		{/if}
	</div>

	<details open={advancedOpen} ontoggle={onFold}>
		<summary class="cursor-pointer text-xs font-medium text-muted-foreground hover:text-foreground">
			Advanced <span class="font-normal">· {advancedLine}</span>
		</summary>
		<div class="mt-3 space-y-4">
			{#if draft.protocol === 'mqtt'}
				{#if !needsTopic}
					<div>
						<label for="{idPrefix}-mqtt-telemetry" class={label}>Telemetry topic</label>
						<input
							id="{idPrefix}-mqtt-telemetry"
							type="text"
							bind:value={draft.mqtt.telemetryTopic}
							placeholder={`${draft.mqtt.topicPrefix.replace(/\/+$/, '')}/telemetry`}
							class={input}
						/>
						<p class="mt-1 text-xs text-muted-foreground">
							Only when its data arrives on a topic of its own at the server's broker (a bridged
							broker, Tasmota's
							<code>tele/…/SENSOR</code>). Empty: the prefix's.
						</p>
					</div>
				{/if}
				<div>
					<label for="{idPrefix}-mqtt-qos" class={label}>QoS</label>
					<select id="{idPrefix}-mqtt-qos" bind:value={draft.mqtt.qos} class={input}>
						<option value={0}>0 · at most once</option>
						<option value={1}>1 · at least once</option>
						<option value={2}>2 · exactly once</option>
					</select>
				</div>
			{:else if draft.protocol === 'websocket'}
				<div>
					<label for="{idPrefix}-ws-reconnect" class={label}>Reconnect after (ms)</label>
					<input
						id="{idPrefix}-ws-reconnect"
						type="number"
						bind:value={draft.ws.reconnectMs}
						min={1000}
						step={1000}
						class={input}
					/>
				</div>
			{:else if draft.protocol === 'modbus_tcp'}
				<div class="grid grid-cols-2 gap-4">
					<div>
						<label for="{idPrefix}-tcp-unit" class={label}>Unit ID</label>
						<input
							id="{idPrefix}-tcp-unit"
							type="number"
							bind:value={draft.tcp.unitId}
							min={0}
							max={247}
							class={input}
						/>
					</div>
					<div>
						<label for="{idPrefix}-tcp-poll" class={label}>Poll (ms)</label>
						<input
							id="{idPrefix}-tcp-poll"
							type="number"
							bind:value={draft.tcp.pollIntervalMs}
							min={100}
							step={100}
							class={input}
						/>
					</div>
				</div>
			{:else if draft.protocol === 'modbus_rtu'}
				<div class="grid grid-cols-2 gap-4 sm:grid-cols-4">
					<div>
						<span class="flex items-center gap-1 text-sm font-medium text-foreground">
							<label for="{idPrefix}-rtu-framing">Framing</label>
							<Hint inline help={ASCII_HELP} />
						</span>
						<select id="{idPrefix}-rtu-framing" bind:value={draft.rtu.framing} class={input}>
							<option value="rtu">RTU</option>
							<option value="ascii">ASCII</option>
						</select>
					</div>
					{#if draft.rtu.framing === 'ascii'}
						<div>
							<label for="{idPrefix}-rtu-data-bits" class={label}>Data bits</label>
							<select id="{idPrefix}-rtu-data-bits" bind:value={draft.rtu.dataBits} class={input}>
								<option value={7}>7</option>
								<option value={8}>8</option>
							</select>
						</div>
					{/if}
					<div>
						<label for="{idPrefix}-rtu-stop" class={label}>Stop bits</label>
						<select id="{idPrefix}-rtu-stop" bind:value={draft.rtu.stopBits} class={input}>
							<option value={1}>1</option>
							<option value={2}>2</option>
						</select>
					</div>
					<div>
						<label for="{idPrefix}-rtu-poll" class={label}>Poll (ms)</label>
						<input
							id="{idPrefix}-rtu-poll"
							type="number"
							bind:value={draft.rtu.pollIntervalMs}
							min={100}
							step={100}
							class={input}
						/>
					</div>
				</div>
			{:else if draft.protocol === 'serial_tc4'}
				<Tc4Fields bind:settings={draft.tc4} idPrefix="{idPrefix}-tc4" part="more" />
			{:else if draft.protocol === 'phidget22'}
				<PhidgetFields bind:settings={draft.phidget} idPrefix="{idPrefix}-phidget" part="more" />
			{/if}
		</div>
	</details>
</div>
