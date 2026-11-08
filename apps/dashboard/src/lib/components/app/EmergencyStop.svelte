<script lang="ts">
	/**
	 * The header's safety button for the selected roaster (`lib/roaster/stop.ts`):
	 * STOP (heater off, heater output 0 and the device's e-stop: fan to full, safe state) while
	 * there is something to stop; **Fan off** once the heater is off and the roaster has cooled
	 * below its cool-down temperature but the fan still runs (after an emergency stop); nothing
	 * on a cold idle roaster, a silent one, or one whose control is off (docs/roaster-setup.md).
	 */
	import { control } from '#lib/api/control.js';
	import { roastSequence } from '#lib/api/sequence.js';
	import { roast, controlLevel } from '#lib/stores/roast.svelte.js';
	import { telemetry } from '#lib/stores/telemetry.js';
	import { allows } from '#lib/roaster/setup.js';
	import { stopButton } from '#lib/roaster/stop.js';
	import { notifications } from '#lib/stores/notifications.js';
	import { OctagonX, Fan } from '@lucide/svelte';
	import type { Telemetry } from '#lib/api/telemetry.js';

	let { size = 'md' }: { size?: 'md' | 'lg' } = $props();

	let stopping = $state(false);
	let fanning = $state(false);
	const dev = $derived(roast.deviceId);
	const may = $derived(allows(controlLevel(), 'stop'));

	// The newest reading and when it came (this browser's clock), and a clock that ticks so a
	// silent roaster hides the button
	let reading = $state<Telemetry | null>(null);
	let heardAt = $state<number | null>(null);
	let now = $state(Date.now());
	$effect(() =>
		telemetry.subscribe((t) => {
			reading = t;
			heardAt = t ? Date.now() : null;
		})
	);
	$effect(() => {
		const tick = setInterval(() => (now = Date.now()), 2000);
		return () => clearInterval(tick);
	});

	// The roast sequence's cool-down temperature and whether the server is cooling after DROP:
	// read for the roaster, and again every 15 s while its fan runs (that is when it matters)
	let coolEndC = $state<number | null>(null);
	let cooling = $state(false);
	const fanOn = $derived((reading?.fan ?? 0) > 0);
	$effect(() => {
		const id = dev;
		coolEndC = null;
		cooling = false;
		if (!id) return;
		const load = () =>
			roastSequence
				.get(id)
				.then((v) => {
					if (id !== roast.deviceId) return;
					coolEndC = v.config.cool_end_bt_c;
					cooling = v.cooling;
				})
				.catch(() => {});
		void load();
		if (!fanOn) return;
		const every = setInterval(load, 15_000);
		return () => clearInterval(every);
	});

	const button = $derived(
		dev
			? stopButton({
					reading,
					heardAt,
					now,
					roastOpen: roast.session != null,
					controllable: may,
					coolEndC,
					cooling
				})
			: null
	);

	async function fanOff() {
		if (!dev) return;
		fanning = true;
		try {
			await control.setFan(dev, 0);
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e);
			notifications.add(`Fan off failed: ${msg}`, 'error');
		} finally {
			fanning = false;
		}
	}

	async function emergencyStop() {
		if (!dev) return;
		stopping = true;
		try {
			await Promise.all([
				control.setHeaterEnable(dev, false),
				control.setHeater(dev, 0),
				control.emergencyStop(dev)
			]);
			notifications.add('Emergency stop sent: heater off, fan to full', 'warning');
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e);
			notifications.add(`Emergency stop error: ${msg}`, 'error');
		} finally {
			stopping = false;
		}
	}
</script>

{#if stopping || button?.kind === 'stop'}
	<button
		onclick={emergencyStop}
		disabled={stopping}
		aria-label="Emergency stop"
		title={`Emergency stop ${dev} (${button?.why ?? 'stopping'}): heater off, fan to full`}
		class="flex shrink-0 items-center justify-center gap-1.5 rounded-full bg-red-600 font-bold tracking-wide text-white shadow-sm hover:bg-red-700 active:bg-red-800 disabled:opacity-40
			{size === 'lg' ? 'h-12 px-5 text-base' : 'h-9 px-3.5 text-sm'}"
	>
		<OctagonX class={size === 'lg' ? 'h-5 w-5' : 'h-4 w-4'} />
		{stopping ? '…' : 'STOP'}
	</button>
{:else if button?.kind === 'fan_off'}
	<button
		onclick={fanOff}
		disabled={fanning}
		aria-label="Fan off"
		title={`Turn ${dev}'s fan off (${button.why})`}
		class="flex shrink-0 items-center justify-center gap-1.5 rounded-full border border-border bg-card font-medium text-foreground shadow-sm hover:bg-accent disabled:opacity-40
			{size === 'lg' ? 'h-12 px-5 text-base' : 'h-9 px-3.5 text-sm'}"
	>
		<Fan class={size === 'lg' ? 'h-5 w-5' : 'h-4 w-4'} />
		{fanning ? '…' : 'Fan off'}
	</button>
{/if}
