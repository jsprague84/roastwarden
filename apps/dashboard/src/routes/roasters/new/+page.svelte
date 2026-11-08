<script lang="ts">
	/**
	 * Add roaster (docs/ui-structure.md): the common path first, in four steps.
	 * 1 Roaster: its name (the id follows), its kind; a known machine from Artisan's catalogue fills
	 *   the rest (docs/artisan-machines.md). 2 Connection: how it connects (ConnectionFields), a
	 *   Modbus roaster's registers; Next saves the roaster with control off. 3 Check: the live check
	 *   (CheckView), the mapping editor when it reads nothing or the wrong thing. 4 Control: off,
	 *   manual or full. Leaving after step 2 leaves a roaster that logs only.
	 */
	import { resolve } from '$app/paths';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import {
		addConnection,
		createDevice,
		getDevice,
		getDiscoveredDevices,
		listArtisanMachines,
		planArtisanMachine,
		removeConnection,
		setRegisterMap,
		testConnection,
		updateConnection,
		updateDevice
	} from '#lib/api/devices.js';
	import { MODBUS_PRESETS } from '#lib/constants/modbus-presets.js';
	import type {
		ArtisanMachine,
		DeviceConnection,
		PlanArtisanRequest,
		TestConnectionResponse
	} from '#lib/api/devices.js';
	import {
		connectionConfigFromPlan,
		declaredSummary,
		filterMachines,
		needLabel,
		planExtras,
		prefillFromPlan,
		suggestedId,
		type ArtisanPrefill
	} from '#lib/roaster/artisan.js';
	import { ChevronLeft, ChevronRight, Check, Loader2 } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import type { RoasterCheck } from '#lib/api/adding.js';
	import {
		roasterSetup,
		type ControlLevel,
		type RoasterKind,
		type RoasterSetup
	} from '#lib/api/roasters.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { roast, refreshDevices, selectDevice } from '#lib/stores/roast.svelte.js';
	import { isVirtual } from '#lib/roast/live.js';
	import { errorText } from '#lib/autotune/tuning.js';
	import {
		CONTROL_LEVELS,
		CONTROL_LABEL,
		CONTROL_HELP,
		KINDS,
		KIND_LABEL,
		KIND_HELP,
		outputsLine,
		raiseConfirm
	} from '#lib/roaster/setup.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import ConnectionFields from '#lib/components/roasters/ConnectionFields.svelte';
	import CheckView from '#lib/components/roasters/CheckView.svelte';
	import MappingEditor from '#lib/components/roasters/MappingEditor.svelte';
	import RegisterMapEditor from '#lib/components/roasters/RegisterMapEditor.svelte';
	import { draftOf, roleProblem, type RegisterDraft } from '#lib/roaster/registerRoles.js';
	import { isModbus, protocolLabel } from '#lib/roaster/connection.js';
	import { hasMapping } from '#lib/roaster/mapping.js';
	import {
		draftConfig,
		draftProblem,
		followId,
		idFromName,
		mqttPrefixFor,
		newDraft
	} from '#lib/roaster/connectionForm.js';

	const STEPS = ['Roaster', 'Connection', 'Check', 'Control'];
	let step = $state(1);

	// ---- 1 Roaster ----
	const qp = page.url.searchParams;
	let name = $state(qp.get('name') ?? '');
	const initialId = qp.get('device_id') ?? '';
	let deviceId = $state(initialId);
	/** The id stops following the name once typed (or when it is a discovered roaster's own). */
	let idEdited = $state(!!qp.get('device_id'));
	let editingId = $state(false);
	let kind = $state<RoasterKind | null>(null);
	let description = $state('');
	let location = $state('');
	/** A roaster seen on MQTT (pending): set up in place, its id is what it sends. */
	let existingDeviceId = $state<string | null>(null);

	let draft = $state(
		newDraft(initialId, {
			protocol: qp.get('protocol'),
			host: qp.get('host'),
			port: qp.get('port')
		})
	);
	let registers = $state<RegisterDraft[]>(MODBUS_PRESETS[0].registers.map(draftOf));
	let presetId = $state('roastwarden-standard');

	function setId(next: string) {
		followId(draft, deviceId, next);
		deviceId = next;
	}
	function onName(e: Event) {
		name = (e.currentTarget as HTMLInputElement).value;
		if (!idEdited && !registryId) setId(idFromName(name));
	}
	function onId(e: Event) {
		idEdited = true;
		setId((e.currentTarget as HTMLInputElement).value.trim());
	}

	// A known machine (docs/artisan-machines.md): the server plans it from Artisan's setup file.
	let machines = $state<ArtisanMachine[]>([]);
	let machineQuery = $state('');
	let showAllMachines = $state(false);
	let planning = $state(false);
	let planError = $state('');
	let artisan = $state<ArtisanPrefill | null>(null);
	let machineMatches = $derived(
		machineQuery.trim() || showAllMachines
			? filterMachines(machines, machineQuery, showAllMachines).slice(0, 60)
			: []
	);

	onMount(async () => {
		try {
			machines = await listArtisanMachines();
		} catch {
			// The catalogue is optional: the wizard works without it
		}
		if (deviceId) {
			try {
				const existing = (await getDiscoveredDevices()).find((d) => d.device_id === deviceId);
				if (existing) {
					existingDeviceId = existing.id;
					name = name || existing.name;
				}
				// A roaster set up before (or the simulator) keeps its kind
				const now = await roasterSetup.get(deviceId);
				if (now.updated_at || now.fixed) kind = kind ?? now.kind;
			} catch {
				// Not critical
			}
		}
	});

	async function useMachine(req: PlanArtisanRequest, suggest?: ArtisanMachine) {
		planning = true;
		planError = '';
		try {
			const plan = await planArtisanMachine(req);
			const p = prefillFromPlan(plan);
			if (!p) {
				planError = `${plan.machine.name}: ${plan.machine.why ?? 'Roastwarden cannot set it up from this file'}`;
				return;
			}
			artisan = p;
			// A name typed first keeps its id; otherwise the machine names both
			if (!name.trim()) {
				name = p.name;
				if (!idEdited && !registryId) setId(suggestedId(suggest ?? p.machine));
			}
			draft.protocol = p.protocol;
			if (p.tcp) draft.tcp = p.tcp;
			if (p.rtu) draft.rtu = p.rtu;
			if (p.tc4) draft.tc4 = p.tc4;
			if (p.phidget) draft.phidget = p.phidget;
			if (p.ws) draft.ws = { ...p.ws };
			if (p.mqtt)
				draft.mqtt = {
					topicPrefix: mqttPrefixFor(deviceId),
					telemetryTopic: p.mqtt.telemetryTopic,
					qos: p.mqtt.qos
				};
			if (isModbus(p.protocol)) {
				registers = p.registers;
				presetId = '';
			}
			kind = p.kind;
			machineQuery = '';
		} catch (err) {
			planError = errorText(err);
		} finally {
			planning = false;
		}
	}

	async function useAsetFile(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;
		await useMachine({ aset: await file.text(), file_name: file.name });
		input.value = '';
	}

	// ---- 2 Connection ----
	const modbus = $derived(isModbus(draft.protocol));
	/** The plan's needs while the form is still on its protocol. */
	const planNeeds = $derived(artisan && artisan.protocol === draft.protocol ? artisan.needs : []);
	const fromArtisan = $derived(
		artisan && artisan.protocol === draft.protocol && !modbus ? planExtras(artisan.extra) : ''
	);
	const connectionProblem = $derived(
		draftProblem(draft, planNeeds) ?? (modbus ? roleProblem(registers) : null)
	);

	// ---- Saving (end of step 2): the roaster logs only until step 4 ----
	let registryId = $state<string | null>(null);
	let connection = $state<DeviceConnection | null>(null);
	let setup = $state<RoasterSetup | null>(null);
	let saving = $state(false);
	let saveError = $state('');

	async function save(): Promise<boolean> {
		saving = true;
		saveError = '';
		const id = deviceId.trim();
		const details = {
			name: name.trim(),
			description: description.trim() || undefined,
			location: location.trim() || undefined
		};
		try {
			if (!registryId) {
				if (existingDeviceId) {
					await updateDevice(existingDeviceId, { ...details, status: 'active' });
					registryId = existingDeviceId;
				} else {
					const dev = await createDevice({ device_id: id, ...details });
					registryId = dev.id;
					// Created pending: this one is set up on purpose
					await updateDevice(dev.id, { status: 'active' });
				}
			} else {
				await updateDevice(registryId, details);
			}

			// Its connection: the one saved before (or the discovered roaster's own) is changed in
			// place; a different protocol replaces it.
			const config = connectionConfigFromPlan(artisan, draft.protocol, draftConfig(draft));
			const conns = (await getDevice(registryId)).connections ?? [];
			const before = connection
				? conns.find((c) => c.id === connection!.id)
				: existingDeviceId
					? conns.find((c) => c.protocol === draft.protocol)
					: undefined;
			if (before && before.protocol === draft.protocol) {
				connection = await updateConnection(registryId, before.id, { config, enabled: true });
			} else {
				if (before) await removeConnection(registryId, before.id);
				connection = await addConnection(registryId, {
					protocol: draft.protocol,
					enabled: true,
					config
				});
			}
			if (modbus) await setRegisterMap(registryId, registers);

			// Its kind, at control off (a roaster set up before keeps its level)
			try {
				const now = await roasterSetup.get(id);
				const control: ControlLevel = now.updated_at ? now.control : 'off';
				const declared = artisan
					? {
							outputs: artisan.outputs,
							fan_floor_pct: artisan.fan_floor_pct,
							channels: artisan.channels
						}
					: {};
				setup =
					now.fixed || (now.updated_at && now.kind === kind && !artisan)
						? now
						: await roasterSetup.set(id, { kind: kind ?? 'other', control, ...declared });
			} catch (err) {
				notifications.add(`Roaster saved; its kind was not: ${errorText(err)}`, 'error');
			}
			return true;
		} catch (err) {
			saveError = errorText(err);
			return false;
		} finally {
			saving = false;
		}
	}

	// ---- 3 Check ----
	let check = $state<RoasterCheck | null>(null);
	let testing = $state(false);
	let testResult = $state<TestConnectionResponse | null>(null);

	async function test() {
		if (!connection) return;
		testing = true;
		testResult = null;
		try {
			testResult = await testConnection({
				protocol: connection.protocol,
				config: connection.config,
				device_id: deviceId.trim()
			});
		} catch (err) {
			testResult = { success: false, message: errorText(err) };
		} finally {
			testing = false;
		}
	}

	// ---- 4 Control ----
	let control = $state<ControlLevel>('off');
	let useIt = $state(!roast.deviceId || isVirtual(roast.deviceId));
	let finishing = $state(false);

	async function finish() {
		const id = deviceId.trim();
		const from = setup?.control ?? 'off';
		if (control !== from) {
			if (control !== 'off' && !confirm(raiseConfirm(id))) return;
			finishing = true;
			try {
				await roasterSetup.set(id, { kind: setup?.kind ?? kind ?? 'other', control });
			} catch (err) {
				notifications.add(`Control was not changed: ${errorText(err)}`, 'error');
			}
		}
		finishing = true;
		if (useIt) {
			await refreshDevices();
			selectDevice(id);
		}
		// Step 4 follows a save, which sets registryId
		if (registryId) goto(resolve('/roasters/[id]', { id: registryId }));
	}

	// ---- Navigation ----
	const canNext = $derived(
		step === 1
			? name.trim() !== '' && deviceId.trim() !== '' && kind !== null
			: step === 2
				? connectionProblem === null
				: true
	);

	async function next() {
		if (!canNext) return;
		if (step === 2 && !(await save())) return;
		if (step === 3) control = setup?.control ?? 'off';
		step++;
	}
</script>

<svelte:head>
	<title>Add roaster | Roastwarden</title>
</svelte:head>

<div class="mx-auto max-w-3xl space-y-6">
	<div class="flex items-center gap-3">
		<a
			href={resolve('/roasters')}
			class="rounded-md p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
			aria-label="Roasters"
		>
			<ChevronLeft class="h-5 w-5" />
		</a>
		<h1 class="text-2xl font-bold text-foreground">Add roaster</h1>
	</div>

	<ol class="flex items-center gap-2" aria-label="Steps">
		{#each STEPS as label, i (label)}
			{@const n = i + 1}
			<li
				class="flex items-center gap-2 {n < STEPS.length ? 'flex-1' : ''}"
				aria-current={n === step ? 'step' : undefined}
			>
				<span
					class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-semibold
					{n < step
						? 'bg-action text-action-foreground'
						: n === step
							? 'border-2 border-action text-foreground'
							: 'border border-border text-muted-foreground'}"
				>
					{#if n < step}<Check class="h-3.5 w-3.5" />{:else}{n}{/if}
				</span>
				<span
					class="text-sm {n === step
						? 'font-medium text-foreground'
						: 'hidden text-muted-foreground sm:inline'}">{label}</span
				>
				{#if n < STEPS.length}<span class="h-px flex-1 {n < step ? 'bg-action' : 'bg-border'}"
					></span>{/if}
			</li>
		{/each}
	</ol>

	<div class="rounded-lg border border-border bg-card p-4 sm:p-6">
		{#if step === 1}
			<div class="space-y-5">
				<div>
					<label for="roaster-name" class="block text-sm font-medium text-foreground">Name</label>
					<input
						id="roaster-name"
						type="text"
						value={name}
						oninput={onName}
						placeholder="e.g. Home roaster"
						class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
					/>
					<p class="mt-1 flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
						{#if editingId && !existingDeviceId && !registryId}
							<label for="roaster-id">ID</label>
							<input
								id="roaster-id"
								type="text"
								value={deviceId}
								oninput={onId}
								class="w-48 rounded border border-border bg-input px-2 py-0.5 font-mono text-xs text-foreground"
							/>
							<span>what topics and the <code>roast</code> CLI call it</span>
						{:else}
							<span
								>ID <span class="font-mono text-foreground" data-testid="roaster-id"
									>{deviceId || '…'}</span
								></span
							>
							{#if !existingDeviceId && !registryId}
								<button
									type="button"
									onclick={() => (editingId = true)}
									class="underline hover:text-foreground">change</button
								>
							{:else if existingDeviceId}
								<span>· the id it sends</span>
							{/if}
						{/if}
					</p>
				</div>

				<fieldset>
					<legend class="flex items-center gap-1 text-sm font-medium text-foreground"
						>Kind <Hint inline help={KIND_HELP} /></legend
					>
					<div class="mt-2 flex flex-wrap gap-2" role="radiogroup">
						{#each KINDS as k (k)}
							<label
								class="cursor-pointer rounded-md border px-3 py-1.5 text-sm {kind === k
									? 'border-action text-foreground'
									: 'border-border text-muted-foreground hover:text-foreground'}"
							>
								<input
									type="radio"
									name="roaster-kind"
									value={k}
									bind:group={kind}
									class="sr-only"
								/>
								{KIND_LABEL[k]}
							</label>
						{/each}
					</div>
				</fieldset>

				<!-- A known machine (docs/artisan-machines.md) -->
				<div>
					<span class="flex items-center gap-1 text-sm font-medium text-foreground">
						<label for="machine-search">A known machine?</label>
						<Hint
							inline
							help={{
								line: "Pick it from Artisan's machine list and its connection, registers, outputs and channels are filled in.",
								more: "Roastwarden reads Artisan's machine catalogue (or an .aset file saved from Artisan) and plans the roaster: the connection (Modbus RTU or ASCII, Modbus TCP, a TC4, a WebSocket or MQTT with its field mapping and command templates), each Modbus register's role and slave, outputs from Artisan's sliders, channels from its extra curves. Control stays off until you switch it on."
							}}
						/>
					</span>
					{#if artisan}
						<div class="mt-1 space-y-1 text-sm">
							<div class="flex flex-wrap items-baseline gap-x-2">
								<span class="font-medium text-foreground">{artisan.machine.name}</span>
								<span class="text-xs text-muted-foreground"
									>{protocolLabel(artisan.protocol)} · {declaredSummary(artisan)}</span
								>
								<button
									type="button"
									onclick={() => ((artisan = null), (planError = ''))}
									class="text-xs text-muted-foreground underline hover:text-foreground"
								>
									Clear
								</button>
							</div>
							{#if artisan.needs.length}
								<p class="text-xs text-highlight">
									Next step: give the {artisan.needs.map(needLabel).join(' and ')}.
								</p>
							{/if}
							{#if artisan.notes.length}
								<ul class="list-disc space-y-0.5 pl-5 text-xs text-muted-foreground">
									{#each artisan.notes as n (n)}<li>{n}</li>{/each}
								</ul>
							{/if}
							<p class="text-[11px] text-muted-foreground">From {artisan.source}.</p>
						</div>
					{:else}
						<div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1">
							<input
								id="machine-search"
								type="search"
								bind:value={machineQuery}
								placeholder={machines.length
									? `Brand or model, ${machines.filter((m) => m.supported).length} machines`
									: 'Brand or model'}
								class="w-full rounded-md border border-border bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus sm:w-72"
							/>
							<label class="inline-flex items-center gap-1 text-xs text-muted-foreground">
								<input type="checkbox" bind:checked={showAllMachines} /> show all
							</label>
							<label
								class="cursor-pointer text-xs text-muted-foreground underline hover:text-foreground"
							>
								open an .aset file
								<input
									type="file"
									accept=".aset,text/plain"
									class="sr-only"
									onchange={useAsetFile}
								/>
							</label>
							{#if planning}<Loader2 class="h-4 w-4 animate-spin text-muted-foreground" />{/if}
						</div>
						{#if machineMatches.length}
							<ul
								class="mt-2 max-h-48 divide-y divide-border overflow-y-auto rounded-md border border-border text-sm"
							>
								{#each machineMatches as m (m.key)}
									<li>
										<button
											type="button"
											disabled={!m.supported || planning}
											title={m.why ?? `Plan ${m.name} from Artisan's setup`}
											onclick={() => useMachine({ machine: m.key }, m)}
											class="flex w-full items-baseline gap-2 px-3 py-1.5 text-left hover:bg-accent disabled:cursor-not-allowed disabled:hover:bg-transparent"
										>
											<span class={m.supported ? 'text-foreground' : 'text-muted-foreground'}
												>{m.brand} {m.model}</span
											>
											<span class="text-xs text-muted-foreground"
												>{m.supported ? m.artisan_device_name : m.why}</span
											>
										</button>
									</li>
								{/each}
							</ul>
						{:else if machineQuery.trim() && machines.length}
							<p class="mt-1 text-xs text-muted-foreground">
								No machine matches{showAllMachines
									? ''
									: ' (show all lists the ones Roastwarden cannot read yet)'}. Any roaster can be
								set up by hand in the next step.
							</p>
						{/if}
					{/if}
					{#if planError}<p class="mt-1 text-xs text-destructive">{planError}</p>{/if}
				</div>

				{#if !existingDeviceId}
					<p class="text-xs text-muted-foreground">
						Building one with an ESP32? <a
							href={resolve('/roasters/firmware')}
							class="underline hover:text-foreground">Download its firmware</a
						> first: once it runs, the roaster shows up under Found, ready to set up.
					</p>
				{/if}

				<details open={!!(description || location) || undefined}>
					<summary
						class="cursor-pointer text-xs font-medium text-muted-foreground hover:text-foreground"
						>More: description, location</summary
					>
					<div class="mt-3 grid gap-4 sm:grid-cols-2">
						<div>
							<label for="roaster-description" class="block text-sm font-medium text-foreground"
								>Description</label
							>
							<input
								id="roaster-description"
								type="text"
								bind:value={description}
								class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-focus"
							/>
						</div>
						<div>
							<label for="roaster-location" class="block text-sm font-medium text-foreground"
								>Location</label
							>
							<input
								id="roaster-location"
								type="text"
								bind:value={location}
								placeholder="e.g. Garage"
								class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
							/>
						</div>
					</div>
				</details>
			</div>
		{:else if step === 2}
			<div class="space-y-5">
				{#if fromArtisan}
					<p class="text-xs text-muted-foreground">
						From {artisan?.machine.name}: {fromArtisan}. Saved with the connection; edit them later
						under its Mapping.
					</p>
				{/if}
				<ConnectionFields bind:draft idPrefix="new" {deviceId} needs={planNeeds} />
				{#if modbus}
					<div class="border-t border-border pt-4">
						<h2 class="text-sm font-medium text-foreground">Registers</h2>
						<p class="mt-0.5 text-xs text-muted-foreground">
							Which register holds BT, ET and each output. A preset, a known machine's own, or your
							own rows.
						</p>
						<div class="mt-3"><RegisterMapEditor bind:registers bind:presetId /></div>
					</div>
				{/if}
				{#if saveError}<p class="text-sm text-destructive">{saveError}</p>{/if}
			</div>
		{:else if step === 3}
			<div class="space-y-4">
				<p class="text-xs text-muted-foreground">
					Saved with control off: it records and sends nothing. This step only reads, every 3 s; you
					can leave and come back to it under Roasters.
				</p>
				<CheckView
					deviceId={deviceId.trim()}
					every={3000}
					title="Does it read?"
					bind:view={check}
				/>
				{#if check && check.samples === 0}
					<div class="space-y-2 border-t border-border pt-3 text-sm">
						<p class="text-muted-foreground">
							Nothing has arrived yet.
							{#if draft.protocol === 'mqtt'}Is it on, on the server's broker, and sending to <code
									>{draftConfig(draft).telemetry_topic ??
										`${draftConfig(draft).topic_prefix}/telemetry`}</code
								>?{:else if draft.protocol === 'phidget22'}Is Phidgets' network server running on <code
									>{draft.phidget.host.trim()}:{draft.phidget.port}</code
								>, with the board plugged in? The test lists the boards it has.{/if}
						</p>
						<div class="flex flex-wrap items-center gap-3">
							<button
								type="button"
								onclick={test}
								disabled={testing}
								class="inline-flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent disabled:opacity-50"
							>
								{#if testing}<Loader2 class="h-3.5 w-3.5 animate-spin" />{/if}
								Test the connection
							</button>
							{#if testResult}
								<span class="text-sm {testResult.success ? 'text-go' : 'text-destructive'}">
									{testResult.message}{#if testResult.latency_ms != null}&nbsp;({testResult.latency_ms}
										ms){/if}
								</span>
							{/if}
						</div>
					</div>
				{/if}
				{#if connection && registryId && hasMapping(connection.protocol)}
					<details
						class="border-t border-border pt-3"
						open={check?.verdict === 'fail' && (check?.samples ?? 0) > 0 ? true : undefined}
					>
						<summary class="cursor-pointer text-sm font-medium text-foreground"
							>Readings missing or wrong? Map its fields</summary
						>
						<p class="mt-1 text-xs text-muted-foreground">
							Which of its own field names is BT, ET, heater and fan, its units and topics, shown
							against the newest message it sent.
						</p>
						<MappingEditor
							{registryId}
							deviceId={deviceId.trim()}
							conn={connection}
							onsaved={() =>
								void (
									registryId &&
									getDevice(registryId).then(
										(d) =>
											(connection =
												d.connections.find((c) => c.id === connection?.id) ?? connection)
									)
								)}
						/>
					</details>
				{:else if modbus}
					<p class="border-t border-border pt-3 text-xs text-muted-foreground">
						Readings wrong? Go back and give each register its role.
					</p>
				{/if}
			</div>
		{:else}
			<div class="space-y-4">
				<div>
					<span class="text-sm font-medium text-foreground" id="control-label"
						>What may Roastwarden send it?</span
					>
					<div
						class="mt-1 flex flex-wrap gap-x-1 border-b border-border"
						role="radiogroup"
						aria-labelledby="control-label"
					>
						{#each CONTROL_LEVELS as l (l)}
							<span class="flex items-center gap-1">
								<button
									type="button"
									role="radio"
									aria-checked={control === l}
									onclick={() => (control = l)}
									class="-mb-px whitespace-nowrap border-b-2 px-2 py-1.5 text-sm font-medium transition-colors
									{control === l
										? 'border-action text-foreground'
										: 'border-transparent text-muted-foreground hover:text-foreground'}"
								>
									{CONTROL_LABEL[l]}
								</button>
								<Hint inline help={CONTROL_HELP[l]} />
							</span>
						{/each}
					</div>
					<Hint class="mt-1.5" help={CONTROL_HELP[control]} />
				</div>
				{#if check && check.verdict === 'fail' && control !== 'off'}
					<p class="text-sm text-highlight">
						The check does not pass yet: leave control off until it reads right.
					</p>
				{/if}
				{#if setup}
					<p class="text-xs text-muted-foreground">
						{KIND_LABEL[setup.kind]} · outputs: {outputsLine(setup.outputs, setup.fan_floor_pct)}.
						Outputs, channels and the roast sequence are on its Control tab.
					</p>
				{/if}
				<label class="flex items-center gap-2 text-sm text-foreground">
					<input type="checkbox" bind:checked={useIt} /> Roast with it in this browser
				</label>
			</div>
		{/if}
	</div>

	{#if step === 2 && connectionProblem}
		<p class="-mb-3 text-right text-xs text-muted-foreground" data-testid="connection-problem">
			{connectionProblem}
		</p>
	{/if}
	<div class="flex items-center justify-between">
		<div>
			{#if step > 1}
				<button
					type="button"
					onclick={() => step--}
					class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-4 py-2 text-sm font-medium text-foreground hover:bg-accent"
				>
					<ChevronLeft class="h-4 w-4" />
					Back
				</button>
			{/if}
		</div>
		<div class="flex items-center gap-3">
			{#if step === 4}
				<button
					type="button"
					onclick={finish}
					disabled={finishing}
					class="inline-flex items-center gap-2 rounded-md bg-action px-5 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>
					{#if finishing}<Loader2 class="h-4 w-4 animate-spin" />{:else}<Check
							class="h-4 w-4"
						/>{/if}
					Finish
				</button>
			{:else}
				<button
					type="button"
					onclick={next}
					disabled={!canNext || saving}
					class="inline-flex items-center gap-1.5 rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>
					{#if saving}<Loader2 class="h-4 w-4 animate-spin" />{/if}
					{step === 2 ? 'Save and check' : 'Next'}
					<ChevronRight class="h-4 w-4" />
				</button>
			{/if}
		</div>
	</div>
</div>
