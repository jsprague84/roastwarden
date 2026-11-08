<script lang="ts">
	/**
	 * A roaster on the Roasters list: what a shop checks before the day's roasts at a
	 * glance: reading or silent, its kind and connection, its control level, and the check's
	 * verdict (refreshed every 10 s). The whole card opens the roaster; the verdict opens its Check.
	 */
	import { resolve } from '$app/paths';
	import { getDevice } from '#lib/api/devices.js';
	import { roasterCheck, type RoasterCheck } from '#lib/api/adding.js';
	import { roasterSetup, type RoasterSetup } from '#lib/api/roasters.js';
	import type { ConfiguredDevice, DeviceConnection } from '#lib/api/devices.js';
	import { connectionSummary, protocolLabel } from '#lib/roaster/connection.js';
	import { CONTROL_LABEL, KIND_LABEL } from '#lib/roaster/setup.js';
	import { statusClass } from '#lib/roaster/check.js';

	let { device, selected = false }: { device: ConfiguredDevice; selected?: boolean } = $props();

	let connections = $state<DeviceConnection[]>([]);
	let setup = $state<RoasterSetup | null>(null);
	let check = $state<RoasterCheck | null>(null);

	$effect(() => {
		const id = device.device_id;
		void getDevice(device.id)
			.then((d) => (connections = d.connections ?? []))
			.catch(() => (connections = []));
		void roasterSetup
			.get(id)
			.then((s) => (setup = s))
			.catch(() => (setup = null));
		const read = () =>
			roasterCheck
				.get(id)
				.then((c) => (check = c))
				.catch(() => (check = null));
		void read();
		const t = setInterval(read, 10_000);
		return () => clearInterval(t);
	});

	const reading = $derived(!!check && check.samples > 0);
	const conn = $derived(connections.find((c) => c.enabled) ?? connections[0] ?? null);
	const verdict = $derived(
		!check
			? ''
			: !reading
				? 'no readings'
				: check.verdict === 'ok'
					? 'check passes'
					: check.verdict === 'warn'
						? 'check: look at it'
						: 'check fails'
	);
</script>

<div
	class="relative rounded-lg border border-border bg-card p-4 transition-colors hover:bg-accent"
	data-testid="roaster-card"
>
	<div class="flex items-start justify-between gap-2">
		<div class="min-w-0">
			<a
				href={resolve('/roasters/[id]', { id: device.id })}
				class="truncate text-sm font-semibold text-foreground after:absolute after:inset-0"
				>{device.name}</a
			>
			<p class="mt-0.5 font-mono text-xs text-muted-foreground">{device.device_id}</p>
		</div>
		<span class="flex shrink-0 items-center gap-2 text-xs">
			{#if selected}<span class="text-selected">selected</span>{/if}
			<span
				class="inline-flex items-center gap-1 {reading ? 'text-go' : 'text-muted-foreground'}"
				data-testid="roaster-reading"
			>
				<span class="h-2 w-2 rounded-full {reading ? 'bg-go' : 'bg-muted-foreground/40'}"></span>
				{check ? (reading ? 'reading' : 'silent') : '…'}
			</span>
		</span>
	</div>
	<dl class="mt-3 grid grid-cols-[4.5rem_minmax(0,1fr)] gap-x-2 gap-y-0.5 text-xs">
		<dt class="text-muted-foreground">Kind</dt>
		<dd class="truncate text-foreground">{setup ? KIND_LABEL[setup.kind] : '…'}</dd>
		<dt class="text-muted-foreground">Connects</dt>
		<dd
			class="truncate text-foreground"
			title={conn ? connectionSummary(conn.protocol, conn.config) : undefined}
		>
			{#if conn}{protocolLabel(conn.protocol)}
				<span class="text-muted-foreground">{connectionSummary(conn.protocol, conn.config)}</span
				>{:else}none{/if}
		</dd>
		<dt class="text-muted-foreground">Control</dt>
		<dd class="text-foreground">{setup ? CONTROL_LABEL[setup.control] : '…'}</dd>
	</dl>
	<div class="mt-3 flex items-center justify-between gap-2 text-xs">
		<a
			href={resolve('/roasters/[id]/check', { id: device.id })}
			class="relative z-10 underline-offset-2 hover:underline {check && reading
				? statusClass(check.verdict)
				: 'text-muted-foreground'}"
			data-testid="roaster-verdict">{verdict || 'checking…'}</a
		>
		{#if device.location}<span class="truncate text-muted-foreground">{device.location}</span>{/if}
	</div>
</div>
