<script lang="ts">
	/** A roaster's overview: the device contract, its connections and, for Modbus, the register map (editable). */
	import { refreshAll } from '$app/navigation';
	import {
		addConnection,
		updateConnection,
		removeConnection,
		getRegisterMap,
		setRegisterMap,
		testConnection
	} from '#lib/api/devices.js';
	import {
		draftOf,
		ROLE_LABEL,
		roleProblem,
		type RegisterDraft,
		type RegisterRole
	} from '#lib/roaster/registerRoles.js';
	import RegisterMapEditor from '#lib/components/roasters/RegisterMapEditor.svelte';
	import type {
		ConfiguredDeviceWithConnections,
		DeviceConnection,
		ConnectionProtocol,
		ModbusRegisterMapEntry,
		TestConnectionResponse
	} from '#lib/api/devices.js';
	import {
		Trash2,
		Plus,
		Wifi,
		Database,
		Loader2,
		X,
		Globe,
		Cable,
		Thermometer
	} from '@lucide/svelte';
	import { onMount } from 'svelte';
	import DeviceContract from '#lib/components/roasters/DeviceContract.svelte';
	import RoasterModel from '#lib/components/roasters/RoasterModel.svelte';
	import ConnectionFields from '#lib/components/roasters/ConnectionFields.svelte';
	import MappingEditor from '#lib/components/roasters/MappingEditor.svelte';
	import { hasMapping } from '#lib/roaster/mapping.js';
	import { connectionSummary, isModbus, protocolLabel } from '#lib/roaster/connection.js';
	import { draftConfig, draftProblem, newDraft } from '#lib/roaster/connectionForm.js';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let device = $derived(data.device as ConfiguredDeviceWithConnections);

	// --- Connections: the Add roaster wizard's form (ConnectionFields) ---
	let showAddConnection = $state(false);
	let addDraft = $state(newDraft());
	let addingConnection = $state(false);
	let addConnectionError = $state('');
	const addProblem = $derived(draftProblem(addDraft));

	function resetConnectionForm() {
		addDraft = newDraft(device.device_id);
		addConnectionError = '';
	}

	async function handleAddConnection() {
		addingConnection = true;
		addConnectionError = '';
		try {
			await addConnection(device.id, {
				protocol: addDraft.protocol,
				enabled: true,
				config: draftConfig(addDraft)
			});
			showAddConnection = false;
			resetConnectionForm();
			await refreshAll();
		} catch (err) {
			addConnectionError = err instanceof Error ? err.message : 'Failed to add connection';
		} finally {
			addingConnection = false;
		}
	}

	async function handleToggleConnection(conn: DeviceConnection) {
		try {
			await updateConnection(device.id, conn.id, { enabled: !conn.enabled });
			await refreshAll();
		} catch {
			// silently fail
		}
	}

	let deletingConnectionId = $state<string | null>(null);

	async function handleRemoveConnection(connId: string) {
		deletingConnectionId = connId;
		try {
			await removeConnection(device.id, connId);
			await refreshAll();
		} catch {
			// silently fail
		} finally {
			deletingConnectionId = null;
		}
	}

	// --- Mapping (docs/ingest-mapping.md): one editor open at a time ---
	let mappingOpen = $state<string | null>(null);

	// --- Test connection ---
	let testingConnectionId = $state<string | null>(null);
	let testResults = $state<Record<string, TestConnectionResponse>>({});

	async function handleTestConnection(conn: DeviceConnection) {
		testingConnectionId = conn.id;
		try {
			const result = await testConnection({
				protocol: conn.protocol,
				config: conn.config,
				device_id: device.device_id
			});
			testResults = { ...testResults, [conn.id]: result };
		} catch (err) {
			testResults = {
				...testResults,
				[conn.id]: {
					success: false,
					message: err instanceof Error ? err.message : 'Test failed'
				}
			};
		} finally {
			testingConnectionId = null;
		}
	}

	// --- Register map ---
	let registers = $state<ModbusRegisterMapEntry[]>([]);
	let loadingRegisters = $state(false);

	let hasModbusConnection = $derived(
		device.connections?.some((c) => isModbus(c.protocol)) ?? false
	);

	onMount(async () => {
		if (hasModbusConnection) {
			loadingRegisters = true;
			try {
				registers = await getRegisterMap(device.id);
			} catch {
				// no registers
			} finally {
				loadingRegisters = false;
			}
		}
	});

	// Editing it: the Add wizard's editor on a copy; saving replaces the map and the server
	// restarts the poller with it (docs/ingest-mapping.md, "Modbus TCP and RTU").
	let draft = $state<RegisterDraft[] | null>(null);
	let savingRegisters = $state(false);
	let registerError = $state('');

	async function saveRegisters() {
		if (!draft) return;
		savingRegisters = true;
		registerError = '';
		try {
			await setRegisterMap(device.id, draft);
			registers = await getRegisterMap(device.id);
			draft = null;
		} catch (err) {
			registerError = err instanceof Error ? err.message : 'Failed to save the register map';
		} finally {
			savingRegisters = false;
		}
	}

	function protocolIcon(protocol: ConnectionProtocol) {
		if (protocol === 'mqtt') return Wifi;
		if (protocol === 'websocket') return Globe;
		if (protocol === 'serial_tc4' || protocol === 'modbus_rtu') return Cable;
		if (protocol === 'phidget22') return Thermometer;
		return Database;
	}
</script>

<div class="space-y-6">
	{#key device.device_id}
		<DeviceContract deviceId={device.device_id} />
		<RoasterModel deviceId={device.device_id} />
	{/key}

	<!-- Connections section -->
	<div class="rounded-lg border border-border bg-card p-6">
		<div class="flex items-center justify-between">
			<h2 class="text-lg font-semibold text-foreground">Connections</h2>
			<button
				type="button"
				onclick={() => {
					resetConnectionForm();
					showAddConnection = !showAddConnection;
				}}
				class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
			>
				{#if showAddConnection}
					<X class="h-3.5 w-3.5" />
					Cancel
				{:else}
					<Plus class="h-3.5 w-3.5" />
					Add Connection
				{/if}
			</button>
		</div>

		<!-- Add connection form -->
		{#if showAddConnection}
			<div class="mt-4 space-y-4 rounded-md border border-border p-4">
				<ConnectionFields bind:draft={addDraft} idPrefix="add" deviceId={device.device_id} />

				{#if addConnectionError}
					<div
						class="rounded-md border border-red-500/30 bg-red-500/10 px-4 py-2 text-sm text-red-400"
					>
						{addConnectionError}
					</div>
				{/if}

				<button
					type="button"
					onclick={handleAddConnection}
					disabled={addingConnection || addProblem !== null}
					title={addProblem ?? undefined}
					class="inline-flex items-center gap-1.5 rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>
					{#if addingConnection}
						<Loader2 class="h-3.5 w-3.5 animate-spin" />
					{/if}
					Add Connection
				</button>
			</div>
		{/if}

		<!-- Connection list -->
		{#if device.connections && device.connections.length > 0}
			<div class="mt-4 space-y-3">
				{#each device.connections as conn (conn.id)}
					{@const Icon = protocolIcon(conn.protocol)}
					<div class="rounded-md border border-border bg-muted/20 p-4">
						<div class="flex flex-wrap items-center justify-between gap-2">
							<div class="flex min-w-0 items-center gap-3">
								<Icon class="h-4 w-4 text-muted-foreground" />
								<div>
									<span class="text-sm font-medium text-foreground">
										{protocolLabel(conn.protocol)}
									</span>
									<span class="ml-2 text-xs text-muted-foreground">
										{connectionSummary(conn.protocol, conn.config)}
									</span>
								</div>
							</div>
							<div class="flex items-center gap-2">
								{#if hasMapping(conn.protocol)}
									<button
										type="button"
										onclick={() => (mappingOpen = mappingOpen === conn.id ? null : conn.id)}
										aria-expanded={mappingOpen === conn.id}
										class="rounded-md border bg-card px-2.5 py-1 text-xs text-foreground hover:bg-accent {mappingOpen ===
										conn.id
											? 'border-action'
											: 'border-border'}"
									>
										Mapping
									</button>
								{/if}
								<!-- Test button -->
								<button
									type="button"
									onclick={() => handleTestConnection(conn)}
									disabled={testingConnectionId === conn.id}
									class="rounded-md border border-border bg-card px-2.5 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50"
								>
									{#if testingConnectionId === conn.id}
										<Loader2 class="inline h-3 w-3 animate-spin" />
									{:else}
										Test
									{/if}
								</button>
								<!-- Enabled toggle -->
								<button
									type="button"
									onclick={() => handleToggleConnection(conn)}
									class="rounded-md px-2.5 py-1 text-xs font-medium {conn.enabled
										? 'bg-green-500/15 text-green-400'
										: 'bg-muted text-muted-foreground'}"
								>
									{conn.enabled ? 'Enabled' : 'Disabled'}
								</button>
								<!-- Delete -->
								<button
									type="button"
									onclick={() => handleRemoveConnection(conn.id)}
									disabled={deletingConnectionId === conn.id}
									class="rounded-md p-1.5 text-red-400 hover:bg-red-500/10 disabled:opacity-50"
								>
									{#if deletingConnectionId === conn.id}
										<Loader2 class="h-3.5 w-3.5 animate-spin" />
									{:else}
										<Trash2 class="h-3.5 w-3.5" />
									{/if}
								</button>
							</div>
						</div>
						{#if mappingOpen === conn.id && hasMapping(conn.protocol)}
							<MappingEditor
								registryId={device.id}
								deviceId={device.device_id}
								{conn}
								onsaved={() => void refreshAll()}
							/>
						{:else if isModbus(conn.protocol)}
							<p class="mt-2 text-xs text-muted-foreground">
								Modbus reads by register: give each register its role in the register map (the Role
								column).
							</p>
						{/if}
						<!-- Test result -->
						{#if testResults[conn.id]}
							{@const result = testResults[conn.id]}
							<div class="mt-2 text-xs {result.success ? 'text-green-400' : 'text-red-400'}">
								{result.message}
								{#if result.latency_ms != null}
									({result.latency_ms}ms)
								{/if}
							</div>
						{/if}
					</div>
				{/each}
			</div>
		{:else if !showAddConnection}
			<p class="mt-4 text-sm text-muted-foreground">
				No connections configured. Add a connection to communicate with this device.
			</p>
		{/if}
	</div>

	<!-- Register Map section -->
	{#if hasModbusConnection}
		<div class="rounded-lg border border-border bg-card p-6">
			<div class="flex items-center justify-between">
				<h2 class="text-lg font-semibold text-foreground">Register Map</h2>
				{#if !draft && !loadingRegisters}
					<button
						type="button"
						onclick={() => {
							draft = registers.map(draftOf);
							registerError = '';
						}}
						class="rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
					>
						Edit
					</button>
				{/if}
			</div>
			{#if draft}
				<div class="mt-4 space-y-3">
					<RegisterMapEditor bind:registers={draft} />
					{#if registerError}<p class="text-xs text-red-400">{registerError}</p>{/if}
					<div class="flex gap-2">
						<button
							type="button"
							onclick={saveRegisters}
							disabled={savingRegisters || roleProblem(draft) !== null}
							class="inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-sm text-action-foreground disabled:opacity-50"
						>
							{#if savingRegisters}<Loader2 class="h-3.5 w-3.5 animate-spin" />{/if}
							Save
						</button>
						<button
							type="button"
							onclick={() => (draft = null)}
							class="rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
						>
							Cancel
						</button>
					</div>
				</div>
			{:else if loadingRegisters}
				<div class="mt-4 flex items-center gap-2 text-sm text-muted-foreground">
					<Loader2 class="h-4 w-4 animate-spin" />
					Loading registers...
				</div>
			{:else if registers.length > 0}
				<div class="mt-4 overflow-x-auto rounded-md border border-border">
					<table class="w-full text-left text-sm">
						<thead class="border-b border-border bg-muted/50">
							<tr>
								<th class="px-3 py-2 font-medium text-muted-foreground">Address</th>
								<th class="px-3 py-2 font-medium text-muted-foreground">Name</th>
								<th class="px-3 py-2 font-medium text-muted-foreground">Type</th>
								<th class="px-3 py-2 font-medium text-muted-foreground">Data Type</th>
								<th class="px-3 py-2 font-medium text-muted-foreground">Unit</th>
								<th class="px-3 py-2 font-medium text-muted-foreground">Writable</th>
								<th class="px-3 py-2 font-medium text-muted-foreground">Role</th>
							</tr>
						</thead>
						<tbody class="divide-y divide-border">
							{#each registers as reg, i (i)}
								<tr class="text-foreground">
									<td class="px-3 py-2 font-mono text-xs"
										>0x{reg.address
											.toString(16)
											.padStart(4, '0')
											.toUpperCase()}{#if reg.unit_id}<span
												class="text-muted-foreground"
												title="On slave {reg.unit_id} of the bus"
											>
												@{reg.unit_id}</span
											>{/if}</td
									>
									<td class="px-3 py-2 text-xs">{reg.name}</td>
									<td class="px-3 py-2 text-xs">{reg.register_type}</td>
									<td class="px-3 py-2 text-xs">{reg.data_type}</td>
									<td class="px-3 py-2 text-xs">{reg.unit ?? ''}</td>
									<td class="px-3 py-2 text-xs">{reg.writable ? 'RW' : 'R'}</td>
									<td class="px-3 py-2 text-xs"
										>{reg.role ? (ROLE_LABEL[reg.role as RegisterRole] ?? reg.role) : '—'}</td
									>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{:else}
				<p class="mt-4 text-sm text-muted-foreground">No registers configured for this device.</p>
			{/if}
		</div>
	{/if}
</div>
