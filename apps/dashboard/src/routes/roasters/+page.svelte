<script lang="ts">
	import { resolve } from '$app/paths';
	import type { PageProps } from './$types';
	import { Cpu, Plus } from '@lucide/svelte';
	import type { ConfiguredDevice } from '#lib/api/devices.js';
	import { refreshAll } from '$app/navigation';
	import { updateDevice } from '#lib/api/devices.js';
	import { roast, refreshDevices } from '#lib/stores/roast.svelte.js';
	import { notifications } from '#lib/stores/notifications.js';
	import VirtualRoasterPanel from '#lib/components/roast/VirtualRoasterPanel.svelte';
	import RoasterCard from '#lib/components/roasters/RoasterCard.svelte';
	import { protocolLabel } from '#lib/roaster/connection.js';
	import { isProtocol } from '#lib/roaster/connectionForm.js';

	async function activate(d: ConfiguredDevice) {
		try {
			await updateDevice(d.id, { status: 'active' });
			await Promise.all([refreshAll(), refreshDevices()]);
			notifications.add(`${d.name} activated`, 'success');
		} catch (e) {
			notifications.add(`Activate failed: ${e instanceof Error ? e.message : e}`, 'error');
		}
	}

	let { data }: PageProps = $props();

	let configuredDevices = $derived(
		(data.devices as ConfiguredDevice[]).filter((d) => d.status !== 'pending')
	);

	let pendingDevices = $derived(data.discovered as ConfiguredDevice[]);

	interface MdnsService {
		name: string;
		service_type: string;
		host: string;
		addresses: string[];
		port: number;
		txt: Record<string, string>;
		device_id: string;
		protocol: string;
		last_seen: string;
	}
	let mdns = $derived(data.mdns as { enabled: boolean; services: MdnsService[] });
	let knownIds = $derived(new Set((data.devices as ConfiguredDevice[]).map((d) => d.device_id)));
	/** Seen on the network and not set up yet; a pending roaster already streams on MQTT. */
	let found = $derived(mdns.enabled ? mdns.services.filter((s) => !knownIds.has(s.device_id)) : []);

	/** Roasters → Add's query, filled in from a found roaster. */
	function configureQuery(svc: MdnsService): string {
		const q = new URLSearchParams({
			device_id: svc.device_id,
			name: svc.name,
			protocol: svc.protocol,
			host: svc.addresses[0] ?? svc.host,
			port: String(svc.port)
		});
		return q.toString();
	}
</script>

<svelte:head>
	<title>Roasters | Roastwarden</title>
</svelte:head>

<div class="space-y-5">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<h1 class="text-2xl font-bold text-foreground">Roasters</h1>
		<span class="flex flex-wrap items-center gap-3">
			<a
				href={resolve('/roasters/passwords')}
				class="text-sm text-muted-foreground underline hover:text-foreground"
				data-testid="roasters-passwords">MQTT passwords</a
			>
			<a
				href={resolve('/roasters/firmware')}
				class="text-sm text-muted-foreground underline hover:text-foreground"
				data-testid="roasters-firmware">ESP32 firmware</a
			>
			<a
				href={resolve('/roasters/new')}
				class="inline-flex items-center gap-1.5 rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover"
			>
				<Plus class="h-4 w-4" />
				Add roaster
			</a>
		</span>
	</div>

	<!-- Found and not added yet: streaming on MQTT (pending), or seen with mDNS -->
	{#if pendingDevices.length > 0 || found.length > 0}
		<section
			class="rounded-lg border border-border bg-card p-4"
			aria-labelledby="found-title"
			data-testid="roasters-found"
		>
			<h2 id="found-title" class="text-sm font-semibold text-foreground">Found, not added yet</h2>
			<p class="mt-0.5 text-xs text-muted-foreground">
				Nothing is sent to them. One already streaming on MQTT can be picked as it is: Add keeps it
				as it is, Set up walks through its kind, connection and check.
			</p>
			<ul class="mt-2 divide-y divide-border">
				{#each pendingDevices as device (device.id)}
					<li class="flex flex-wrap items-center justify-between gap-2 py-2">
						<span class="min-w-0 text-sm">
							<span class="font-medium text-foreground">{device.name}</span>
							<span class="ml-2 text-xs text-muted-foreground"
								>MQTT · {device.device_id} · streaming</span
							>
						</span>
						<span class="flex shrink-0 gap-2">
							<button
								type="button"
								onclick={() => activate(device)}
								class="rounded-md border border-border px-3 py-1 text-xs font-medium text-foreground hover:bg-accent"
							>
								Add
							</button>
							<a
								href={resolve(`/roasters/new?device_id=${encodeURIComponent(device.device_id)}`)}
								class="rounded-md bg-action px-3 py-1 text-xs font-medium text-action-foreground hover:bg-action-hover"
								>Set up</a
							>
						</span>
					</li>
				{/each}
				{#each found as svc (svc.service_type + svc.device_id)}
					<li class="flex flex-wrap items-center justify-between gap-2 py-2">
						<span class="min-w-0 text-sm">
							<span class="font-medium text-foreground">{svc.name}</span>
							<span class="ml-2 text-xs text-muted-foreground">
								{isProtocol(svc.protocol) ? protocolLabel(svc.protocol) : svc.protocol} · {svc
									.addresses[0] ?? svc.host}:{svc.port}{#if svc.txt.version}
									· v{svc.txt.version}{/if} · on the network
							</span>
						</span>
						<a
							href={resolve(`/roasters/new?${configureQuery(svc)}`)}
							class="shrink-0 rounded-md bg-action px-3 py-1 text-xs font-medium text-action-foreground hover:bg-action-hover"
							>Set up</a
						>
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	{#if configuredDevices.length === 0 && pendingDevices.length === 0}
		<div class="mt-10 flex flex-col items-center justify-center text-center">
			<Cpu class="h-12 w-12 text-muted-foreground/50" />
			<p class="mt-4 text-lg font-medium text-muted-foreground">No roasters yet</p>
			<p class="mt-1 text-sm text-muted-foreground">
				Add one, switch on a roaster that sends on MQTT (it shows up above), build one with an ESP32
				and
				<a href={resolve('/roasters/firmware')} class="underline hover:text-foreground"
					>its firmware</a
				>, or try the simulator below.
			</p>
		</div>
	{:else if configuredDevices.length > 0}
		<div class="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
			{#each configuredDevices as device (device.id)}
				<RoasterCard {device} selected={roast.deviceId === device.device_id} />
			{/each}
		</div>
	{/if}

	<!-- The server's simulated roasters: switch them on here, use them like any other roaster. -->
	<div class="max-w-3xl pt-2">
		<VirtualRoasterPanel panelKey="simulator-list" open={false} />
	</div>

	<p class="text-xs text-muted-foreground">
		<a href={resolve('/roasters/templates')} class="underline hover:text-foreground"
			>Connection templates</a
		>: saved defaults (mode, setpoint, PID gains, limits) a roaster can name.
	</p>
</div>
