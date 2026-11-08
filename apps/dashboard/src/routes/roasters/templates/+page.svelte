<script lang="ts">
	import { resolve } from '$app/paths';
	import { refreshAll } from '$app/navigation';
	import { toDisplay, fromDisplay, tempUnit, roundForInput } from '#lib/stores/units.svelte.js';
	import { createDeviceProfile, deleteDeviceProfile } from '#lib/api/devices.js';
	import type { DeviceProfile, CreateDeviceProfileRequest } from '#lib/api/devices.js';
	import { Plus, Trash2, ArrowLeft, Settings2, Loader2 } from '@lucide/svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let profiles = $derived(data.profiles as DeviceProfile[]);

	// --- Create form ---
	let showForm = $state(false);
	let creating = $state(false);
	let createError = $state('');

	let formName = $state('');
	let formDescription = $state('');
	let formControlMode = $state('manual');
	let formSetpoint = $state<number | undefined>(undefined);
	let formFan = $state<number | undefined>(undefined);
	let formKp = $state<number | undefined>(undefined);
	let formKi = $state<number | undefined>(undefined);
	let formKd = $state<number | undefined>(undefined);
	let formMaxTemp = $state<number | undefined>(undefined);
	let formMinFan = $state<number | undefined>(undefined);
	let formTelemetryInterval = $state<number | undefined>(undefined);

	function resetForm() {
		formName = '';
		formDescription = '';
		formControlMode = 'manual';
		formSetpoint = undefined;
		formFan = undefined;
		formKp = undefined;
		formKi = undefined;
		formKd = undefined;
		formMaxTemp = undefined;
		formMinFan = undefined;
		formTelemetryInterval = undefined;
		createError = '';
	}

	async function handleCreate() {
		creating = true;
		createError = '';
		try {
			const req: CreateDeviceProfileRequest = {
				name: formName.trim(),
				description: formDescription.trim() || undefined,
				default_control_mode: formControlMode,
				default_setpoint:
					formSetpoint != null ? roundForInput(fromDisplay(formSetpoint), 2) : undefined,
				default_fan: formFan,
				default_kp: formKp,
				default_ki: formKi,
				default_kd: formKd,
				max_temp: formMaxTemp != null ? roundForInput(fromDisplay(formMaxTemp), 2) : undefined,
				min_fan: formMinFan,
				telemetry_interval_ms: formTelemetryInterval
			};
			await createDeviceProfile(req);
			showForm = false;
			resetForm();
			await refreshAll();
		} catch (err) {
			createError = err instanceof Error ? err.message : 'Failed to create profile';
		} finally {
			creating = false;
		}
	}

	// --- Delete ---
	let deletingId = $state<string | null>(null);
	let deleteConfirmId = $state<string | null>(null);

	async function handleDelete(id: string) {
		deletingId = id;
		try {
			await deleteDeviceProfile(id);
			deleteConfirmId = null;
			await refreshAll();
		} catch {
			// silently fail
		} finally {
			deletingId = null;
		}
	}

	// --- Helpers ---
	function profileSummary(p: DeviceProfile): string {
		const parts: string[] = [];
		if (p.default_setpoint != null)
			parts.push(`Setpoint: ${roundForInput(toDisplay(p.default_setpoint))}${tempUnit()}`);
		if (p.default_control_mode) parts.push(`Mode: ${p.default_control_mode}`);
		if (p.max_temp != null) parts.push(`Max: ${roundForInput(toDisplay(p.max_temp))}${tempUnit()}`);
		if (p.telemetry_interval_ms != null) parts.push(`Interval: ${p.telemetry_interval_ms}ms`);
		return parts.join(' · ') || 'No defaults configured';
	}
</script>

<svelte:head>
	<title>Connection templates | Roastwarden</title>
</svelte:head>

<div class="space-y-4">
	<!-- Back link -->
	<a
		href={resolve('/roasters')}
		class="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
	>
		<ArrowLeft class="h-4 w-4" />
		Roasters
	</a>

	<!-- Header -->
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-foreground">Connection templates</h1>
			<p class="mt-1 max-w-xl text-sm text-muted-foreground">
				Reusable connection settings (protocol, register map) for adding roasters of the same kind.
				Not roast profiles: those are under Profiles.
			</p>
		</div>
		<button
			type="button"
			onclick={() => {
				if (showForm) {
					showForm = false;
					resetForm();
				} else {
					showForm = true;
				}
			}}
			class="inline-flex items-center gap-1.5 rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover"
		>
			<Plus class="h-4 w-4" />
			{showForm ? 'Cancel' : 'Create Profile'}
		</button>
	</div>

	<!-- Create form -->
	{#if showForm}
		<div class="rounded-lg border border-border bg-card p-6">
			<h2 class="text-lg font-semibold text-foreground">New Profile</h2>

			<div class="mt-4 space-y-4">
				<div class="grid gap-4 sm:grid-cols-2">
					<div>
						<label for="profile-name" class="block text-sm font-medium text-foreground">
							Name <span class="text-red-400">*</span>
						</label>
						<input
							id="profile-name"
							type="text"
							bind:value={formName}
							placeholder="e.g. Default Roaster"
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
					<div>
						<label for="profile-mode" class="block text-sm font-medium text-foreground">
							Default Control Mode
						</label>
						<select
							id="profile-mode"
							bind:value={formControlMode}
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						>
							<option value="manual">Manual</option>
							<option value="auto">Auto (PID)</option>
						</select>
					</div>
				</div>

				<div>
					<label for="profile-description" class="block text-sm font-medium text-foreground">
						Description
					</label>
					<textarea
						id="profile-description"
						bind:value={formDescription}
						rows={2}
						placeholder="Optional description"
						class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
					></textarea>
				</div>

				<div class="grid gap-4 sm:grid-cols-3">
					<div>
						<label for="profile-setpoint" class="block text-sm font-medium text-foreground">
							Default Setpoint ({tempUnit()})
						</label>
						<input
							id="profile-setpoint"
							type="number"
							bind:value={formSetpoint}
							placeholder="e.g. {roundForInput(toDisplay(200), 0)}"
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
					<div>
						<label for="profile-fan" class="block text-sm font-medium text-foreground">
							Default Fan (%)
						</label>
						<input
							id="profile-fan"
							type="number"
							bind:value={formFan}
							min={0}
							max={100}
							placeholder="0-100"
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
					<div>
						<label for="profile-max-temp" class="block text-sm font-medium text-foreground">
							Max Temp ({tempUnit()})
						</label>
						<input
							id="profile-max-temp"
							type="number"
							bind:value={formMaxTemp}
							placeholder="e.g. {roundForInput(toDisplay(240), 0)}"
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
				</div>

				<div class="grid gap-4 sm:grid-cols-3">
					<div>
						<label for="profile-kp" class="block text-sm font-medium text-foreground">
							PID Kp
						</label>
						<input
							id="profile-kp"
							type="number"
							bind:value={formKp}
							step="0.1"
							placeholder="e.g. 2.0"
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
					<div>
						<label for="profile-ki" class="block text-sm font-medium text-foreground">
							PID Ki
						</label>
						<input
							id="profile-ki"
							type="number"
							bind:value={formKi}
							step="0.01"
							placeholder="e.g. 0.5"
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
					<div>
						<label for="profile-kd" class="block text-sm font-medium text-foreground">
							PID Kd
						</label>
						<input
							id="profile-kd"
							type="number"
							bind:value={formKd}
							step="0.1"
							placeholder="e.g. 1.0"
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
				</div>

				<div class="grid gap-4 sm:grid-cols-2">
					<div>
						<label for="profile-min-fan" class="block text-sm font-medium text-foreground">
							Min Fan (%)
						</label>
						<input
							id="profile-min-fan"
							type="number"
							bind:value={formMinFan}
							min={0}
							max={100}
							placeholder="0-100"
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
					<div>
						<label for="profile-telemetry" class="block text-sm font-medium text-foreground">
							Telemetry Interval (ms)
						</label>
						<input
							id="profile-telemetry"
							type="number"
							bind:value={formTelemetryInterval}
							min={100}
							step={100}
							placeholder="e.g. 1000"
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
				</div>

				{#if createError}
					<div
						class="rounded-md border border-red-500/30 bg-red-500/10 px-4 py-2 text-sm text-red-400"
					>
						{createError}
					</div>
				{/if}

				<button
					type="button"
					onclick={handleCreate}
					disabled={creating || !formName.trim()}
					class="inline-flex items-center gap-1.5 rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
				>
					{#if creating}
						<Loader2 class="h-3.5 w-3.5 animate-spin" />
					{/if}
					Create Profile
				</button>
			</div>
		</div>
	{/if}

	<!-- Profile cards grid -->
	{#if profiles.length === 0 && !showForm}
		<div class="mt-12 flex flex-col items-center justify-center text-center">
			<Settings2 class="h-12 w-12 text-muted-foreground/50" />
			<p class="mt-4 text-lg font-medium text-muted-foreground">No connection templates</p>
			<p class="mt-1 text-sm text-muted-foreground">
				Create a profile to define reusable device configurations.
			</p>
			<button
				type="button"
				onclick={() => (showForm = true)}
				class="mt-4 inline-flex items-center gap-1.5 rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover"
			>
				<Plus class="h-4 w-4" />
				Create Profile
			</button>
		</div>
	{:else if profiles.length > 0}
		<div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
			{#each profiles as profile (profile.id)}
				<div class="rounded-lg border border-border bg-card p-4">
					<div class="flex items-start justify-between">
						<div class="min-w-0 flex-1">
							<div class="flex items-center gap-2">
								<Settings2 class="h-4 w-4 shrink-0 text-amber-400" />
								<h3 class="truncate text-sm font-semibold text-foreground">
									{profile.name}
								</h3>
							</div>
							{#if profile.description}
								<p class="mt-1 line-clamp-2 text-xs text-muted-foreground">
									{profile.description}
								</p>
							{/if}
						</div>
						{#if deleteConfirmId === profile.id}
							<div class="ml-2 flex items-center gap-1">
								<button
									type="button"
									onclick={() => handleDelete(profile.id)}
									disabled={deletingId === profile.id}
									class="rounded-md bg-red-600 px-2 py-1 text-xs font-medium text-white hover:bg-red-700 disabled:opacity-50"
								>
									{#if deletingId === profile.id}
										<Loader2 class="inline h-3 w-3 animate-spin" />
									{:else}
										Confirm
									{/if}
								</button>
								<button
									type="button"
									onclick={() => (deleteConfirmId = null)}
									class="rounded-md border border-border px-2 py-1 text-xs text-foreground hover:bg-accent"
								>
									Cancel
								</button>
							</div>
						{:else}
							<button
								type="button"
								onclick={() => (deleteConfirmId = profile.id)}
								class="ml-2 shrink-0 rounded-md p-1.5 text-red-400 hover:bg-red-500/10"
							>
								<Trash2 class="h-3.5 w-3.5" />
							</button>
						{/if}
					</div>

					<div class="mt-3 text-xs text-muted-foreground">
						{profileSummary(profile)}
					</div>

					{#if profile.default_fan != null || profile.min_fan != null}
						<div class="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground">
							{#if profile.default_fan != null}
								<span class="rounded-full bg-muted px-2 py-0.5">Fan: {profile.default_fan} %</span>
							{/if}
							{#if profile.min_fan != null}
								<span class="rounded-full bg-muted px-2 py-0.5">Min Fan: {profile.min_fan} %</span>
							{/if}
						</div>
					{/if}

					{#if profile.default_kp != null || profile.default_ki != null || profile.default_kd != null}
						<div class="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground">
							{#if profile.default_kp != null}
								<span class="rounded-full bg-muted px-2 py-0.5">Kp: {profile.default_kp}</span>
							{/if}
							{#if profile.default_ki != null}
								<span class="rounded-full bg-muted px-2 py-0.5">Ki: {profile.default_ki}</span>
							{/if}
							{#if profile.default_kd != null}
								<span class="rounded-full bg-muted px-2 py-0.5">Kd: {profile.default_kd}</span>
							{/if}
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>
