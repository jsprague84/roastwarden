<script lang="ts">
	/**
	 * A roaster's pages (docs/ui-structure.md): its header, edit and delete,
	 * and the tabs Overview, Check, Control, Autotune and Simulator, each its own URL.
	 */
	import { resolve } from '$app/paths';
	import { goto, refreshAll } from '$app/navigation';
	import { page } from '$app/state';
	import { updateDevice, deleteDevice } from '#lib/api/devices.js';
	import type { ConfiguredDeviceWithConnections } from '#lib/api/devices.js';
	import { Pencil, Trash2, ArrowLeft, Loader2, Check } from '@lucide/svelte';
	import { roast, selectDevice } from '#lib/stores/roast.svelte.js';
	import { isVirtual } from '#lib/roast/live.js';
	import RoasterSetupPanel from '#lib/components/roasters/RoasterSetupPanel.svelte';
	import type { LayoutProps } from './$types';
	import { isOn } from '#lib/stores/modules.svelte.js';
	import { roasterSetup, type RoasterSetup } from '#lib/api/roasters.js';
	import { CONTROL_LABEL, KIND_LABEL } from '#lib/roaster/setup.js';

	let { data, children }: LayoutProps = $props();

	let device = $derived(data.device as ConfiguredDeviceWithConnections);

	const params = $derived({ id: device.id });
	const tabs = $derived([
		{ href: resolve('/roasters/[id]', params), label: 'Overview', tuning: false },
		{ href: resolve('/roasters/[id]/check', params), label: 'Check', tuning: false },
		{ href: resolve('/roasters/[id]/control', params), label: 'Control', tuning: true },
		...(isOn('tuning')
			? [{ href: resolve('/roasters/[id]/autotune', params), label: 'Autotune', tuning: true }]
			: []),
		...(isVirtual(device.device_id)
			? [{ href: resolve('/roasters/[id]/simulator', params), label: 'Simulator', tuning: false }]
			: [])
	]);
	const current = $derived(tabs.find((t) => t.href === page.url.pathname) ?? tabs[0]);
	/** Tuning panels act on the roaster every page shares (live values come from it). */
	const isSelected = $derived(roast.deviceId === device.device_id);

	/** Its kind and control level for the header; read again on each tab (Control changes them). */
	let setup = $state<RoasterSetup | null>(null);
	$effect(() => {
		const id = device.device_id;
		void page.url.pathname;
		roasterSetup
			.get(id)
			.then((s) => {
				if (id === device.device_id) setup = s;
			})
			.catch(() => (setup = null));
	});

	// --- Edit mode ---
	let editing = $state(false);
	let editName = $state('');
	let editDescription = $state('');
	let editLocation = $state('');
	let editStatus = $state('');
	let saving = $state(false);
	let saveError = $state('');

	function startEdit() {
		editName = device.name;
		editDescription = device.description ?? '';
		editLocation = device.location ?? '';
		editStatus = device.status;
		saveError = '';
		editing = true;
	}

	function cancelEdit() {
		editing = false;
		saveError = '';
		showDeleteConfirm = false;
	}

	async function handleSave() {
		saving = true;
		saveError = '';
		try {
			await updateDevice(device.id, {
				name: editName.trim() || undefined,
				description: editDescription.trim() || undefined,
				location: editLocation.trim() || undefined,
				status: editStatus as ConfiguredDeviceWithConnections['status']
			});
			editing = false;
			await refreshAll();
		} catch (err) {
			saveError = err instanceof Error ? err.message : 'Failed to save';
		} finally {
			saving = false;
		}
	}

	// --- Delete device ---
	let showDeleteConfirm = $state(false);
	let deleting = $state(false);

	async function handleDelete() {
		deleting = true;
		try {
			await deleteDevice(device.id);
			goto(resolve('/roasters'));
		} catch {
			deleting = false;
			showDeleteConfirm = false;
		}
	}

	function relativeTime(iso: string | null | undefined): string {
		if (!iso) return 'Never';
		const diff = Date.now() - new Date(iso).getTime();
		if (diff < 0) return 'Just now';
		const secs = Math.floor(diff / 1000);
		if (secs < 60) return `${secs}s ago`;
		const mins = Math.floor(secs / 60);
		if (mins < 60) return `${mins}m ago`;
		const hours = Math.floor(mins / 60);
		if (hours < 24) return `${hours}h ago`;
		const days = Math.floor(hours / 24);
		return `${days}d ago`;
	}

	function statusClasses(status: string): string {
		switch (status) {
			case 'active':
				return 'bg-green-500/15 text-green-400';
			case 'pending':
				return 'bg-amber-500/15 text-amber-400';
			case 'error':
				return 'bg-red-500/15 text-red-400';
			default:
				return 'bg-muted text-muted-foreground';
		}
	}
</script>

<svelte:head>
	<title
		>{current.label === 'Overview' ? device.name : `${device.name} · ${current.label}`} | Roastwarden</title
	>
</svelte:head>

<div class="space-y-6">
	<a
		href={resolve('/roasters')}
		class="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
	>
		<ArrowLeft class="h-4 w-4" />
		Roasters
	</a>

	<!-- Header section -->
	<div class="rounded-lg border border-border bg-card p-6">
		{#if editing}
			<!-- Edit mode -->
			<div class="space-y-4">
				<div class="flex items-center justify-between">
					<h2 class="text-lg font-semibold text-foreground">Edit Device</h2>
					<div class="flex items-center gap-2">
						<button
							type="button"
							onclick={cancelEdit}
							class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
						>
							Cancel
						</button>
						<button
							type="button"
							onclick={handleSave}
							disabled={saving || !editName.trim()}
							class="inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
						>
							{#if saving}
								<Loader2 class="h-3.5 w-3.5 animate-spin" />
							{:else}
								<Check class="h-3.5 w-3.5" />
							{/if}
							Save
						</button>
					</div>
				</div>

				{#if saveError}
					<div
						class="rounded-md border border-red-500/30 bg-red-500/10 px-4 py-2 text-sm text-red-400"
					>
						{saveError}
					</div>
				{/if}

				<div class="grid gap-4 sm:grid-cols-2">
					<div>
						<label for="edit-name" class="block text-sm font-medium text-foreground">
							Name <span class="text-red-400">*</span>
						</label>
						<input
							id="edit-name"
							type="text"
							bind:value={editName}
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
					<div>
						<label for="edit-status" class="block text-sm font-medium text-foreground">
							Status
						</label>
						<select
							id="edit-status"
							bind:value={editStatus}
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						>
							<option value="active">Active</option>
							<option value="pending">Pending</option>
							<option value="disabled">Disabled</option>
							<option value="error">Error</option>
						</select>
					</div>
					<div class="sm:col-span-2">
						<label for="edit-description" class="block text-sm font-medium text-foreground">
							Description
						</label>
						<textarea
							id="edit-description"
							bind:value={editDescription}
							rows={2}
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						></textarea>
					</div>
					<div>
						<label for="edit-location" class="block text-sm font-medium text-foreground">
							Location
						</label>
						<input
							id="edit-location"
							type="text"
							bind:value={editLocation}
							class="mt-1 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus"
						/>
					</div>
				</div>
				<div class="border-t border-border pt-3">
					{#if showDeleteConfirm}
						<p class="text-sm text-destructive">
							Delete <strong>{device.name}</strong>, its connections and register map? Its roasts
							stay in History. This cannot be undone.
						</p>
						<div class="mt-2 flex items-center gap-2">
							<button
								type="button"
								onclick={handleDelete}
								disabled={deleting}
								class="inline-flex items-center gap-1.5 rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
							>
								{#if deleting}<Loader2 class="h-3.5 w-3.5 animate-spin" />{/if}
								Yes, delete
							</button>
							<button
								type="button"
								onclick={() => (showDeleteConfirm = false)}
								class="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent"
							>
								Keep it
							</button>
						</div>
					{:else}
						<button
							type="button"
							onclick={() => (showDeleteConfirm = true)}
							class="inline-flex items-center gap-1.5 text-sm text-destructive hover:underline"
						>
							<Trash2 class="h-3.5 w-3.5" />
							Delete this roaster
						</button>
					{/if}
				</div>
			</div>
		{:else}
			<!-- View mode -->
			<div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
				<div class="min-w-0 flex-1">
					<div class="flex flex-wrap items-center gap-x-3 gap-y-1">
						<h1 class="text-2xl font-bold break-all text-foreground">{device.name}</h1>
						{#if device.status !== 'active'}
							<span
								class="inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium {statusClasses(
									device.status
								)}">{device.status}</span
							>
						{/if}
					</div>
					<p class="mt-1 text-sm text-muted-foreground" data-testid="roaster-summary">
						<span class="font-mono">{device.device_id}</span>{#if setup}&nbsp; · {KIND_LABEL[
								setup.kind
							]} · control
							<a
								href={resolve('/roasters/[id]/control', params)}
								class="text-foreground underline-offset-2 hover:underline"
								>{CONTROL_LABEL[setup.control]}</a
							>{/if}
					</p>
					{#if device.description}
						<p class="mt-2 text-sm text-foreground">{device.description}</p>
					{/if}
					<div class="mt-3 flex flex-wrap items-center gap-4 text-xs text-muted-foreground">
						{#if device.location}
							<span>{device.location}</span>
						{/if}
						<span>Last seen: {relativeTime(device.last_seen_at)}</span>
					</div>
				</div>
				<div class="flex flex-wrap items-center gap-2 sm:ml-4 sm:justify-end">
					{#if isSelected}
						<span
							class="inline-flex items-center gap-1.5 rounded-md bg-amber-500/15 px-3 py-1.5 text-sm font-medium text-amber-400"
							>Selected roaster</span
						>
					{:else}
						<button
							type="button"
							onclick={() => selectDevice(device.device_id)}
							class="inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover"
						>
							Use this roaster
						</button>
					{/if}
					<button
						type="button"
						onclick={startEdit}
						class="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-sm text-foreground hover:bg-accent"
					>
						<Pencil class="h-3.5 w-3.5" />
						Edit
					</button>
				</div>
			</div>
		{/if}
	</div>

	<nav
		class="-mb-2 flex gap-1 overflow-x-auto border-b border-border"
		aria-label="Roaster sections"
	>
		{#each tabs as t (t.href)}
			<!-- eslint-disable svelte/no-navigation-without-resolve -- the tabs above, each a resolve() of this roaster's route -->
			<a
				href={t.href}
				data-sveltekit-noscroll
				aria-current={current.href === t.href ? 'page' : undefined}
				class="-mb-px whitespace-nowrap border-b-2 px-4 py-2 text-sm font-medium transition-colors
				{current.href === t.href
					? 'border-action text-foreground'
					: 'border-transparent text-muted-foreground hover:text-foreground'}"
			>
				{t.label}
			</a>
			<!-- eslint-enable svelte/no-navigation-without-resolve -->
		{/each}
	</nav>

	{#if current.tuning && !isSelected}
		<!-- The setup is the roaster's own: it does not need the roaster selected -->
		{#if current.href === resolve('/roasters/[id]/control', params)}<RoasterSetupPanel
				deviceId={device.device_id}
			/>{/if}
		<div class="rounded-md border border-amber-500/30 bg-amber-500/10 p-4 text-sm">
			<p class="text-foreground">
				Tuning works on the selected roaster, whose live values the panels show. <strong
					>{roast.deviceId ?? 'No roaster'}</strong
				> is selected now.
			</p>
			<button
				type="button"
				onclick={() => selectDevice(device.device_id)}
				class="mt-3 rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover"
			>
				Use {device.name}
			</button>
		</div>
	{:else}
		{@render children()}
	{/if}
</div>
