<script lang="ts">
	/**
	 * First run (docs/lean-home.md, "First run"): opened once instead of the Roast page while
	 * the server was never set up and holds no roast and no bean. One page: what kind of
	 * roasting (a module preset), the temperature and weight units and the time zone. Save goes on to add a
	 * roaster; "Not now" saves Home. Settings → Modules changes it later.
	 */
	import { goto } from '$app/navigation';
	import { settings } from '#lib/api/settings.js';
	import { resolve } from '$app/paths';
	import type { ResolvedPathname } from '$app/types';
	import { listDevices } from '#lib/api/devices.js';
	import { PRESETS, toValue, type Preset } from '#lib/modules/modules.js';
	import { loadModules } from '#lib/stores/modules.svelte.js';
	import {
		unitState,
		setUnit,
		weightState,
		setWeightUnit,
		type TempUnit,
		type WeightUnit
	} from '#lib/stores/units.svelte.js';
	import { isVirtual } from '#lib/roast/live.js';
	import ModulePresets from '#lib/components/settings/ModulePresets.svelte';

	let preset = $state<Preset>('home');
	let unit = $state<TempUnit>(unitState.unit);
	let weight = $state<WeightUnit>(weightState.unit);
	let timeZone = $state(Intl.DateTimeFormat().resolvedOptions().timeZone || '');
	const zones: string[] =
		(Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf?.(
			'timeZone'
		) ?? [];
	let busy = $state(false);
	let error = $state('');

	/** Roasters → Add, or the simulator when it is the only roaster there is. */
	async function next(): Promise<ResolvedPathname> {
		const devices = await listDevices().catch(() => []);
		const sim = devices.find((d) => d.device_id === 'virtual_roaster');
		return sim && devices.every((d) => isVirtual(d.device_id))
			? resolve('/roasters/[id]/simulator', { id: sim.id })
			: resolve('/roasters/new');
	}

	async function finish(values: Record<string, string>, to: () => Promise<ResolvedPathname>) {
		busy = true;
		error = '';
		try {
			await settings.setMany(values);
			await loadModules();
			// eslint-disable-next-line svelte/no-navigation-without-resolve -- to() returns a resolve() result (next, notNow)
			goto(await to());
		} catch (e) {
			error = e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e);
		} finally {
			busy = false;
		}
	}

	function save() {
		setUnit(unit);
		setWeightUnit(weight);
		const values: Record<string, string> = {
			modules: toValue(PRESETS[preset]),
			report_units: unit
		};
		if (timeZone.trim()) values.time_zone = timeZone.trim();
		void finish(values, next);
	}

	/** Home, and on to the Roast page. */
	const notNow = () => finish({ modules: toValue(PRESETS.home) }, async () => resolve('/'));

	const input =
		'rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const seg = 'px-3 py-1.5 text-sm font-medium transition-colors';
</script>

<svelte:head>
	<title>Welcome | Roastwarden</title>
</svelte:head>

<div class="mx-auto max-w-3xl space-y-6">
	<div>
		<h1 class="text-2xl font-bold text-foreground">Welcome to Roastwarden</h1>
		<p class="mt-1 text-sm text-muted-foreground">
			A few questions, then add your roaster. All of it can be changed later in Settings.
		</p>
	</div>

	<section class="space-y-2">
		<h2 class="text-sm font-semibold text-foreground">What kind of roasting?</h2>
		<ModulePresets value={preset} disabled={busy} onpick={(p) => (preset = p)} />
	</section>

	<section class="space-y-2">
		<h2 class="text-sm font-semibold text-foreground">Temperatures in</h2>
		<div
			class="inline-flex overflow-hidden rounded-md border border-border"
			role="radiogroup"
			aria-label="Temperature unit"
		>
			{#each ['C', 'F'] as const as u (u)}
				<button
					type="button"
					role="radio"
					aria-checked={unit === u}
					class="{seg} {unit === u
						? 'bg-selected text-selected-foreground'
						: 'text-muted-foreground hover:bg-accent'}"
					onclick={() => (unit = u)}>°{u}</button
				>
			{/each}
		</div>
	</section>

	<section class="space-y-2">
		<h2 class="text-sm font-semibold text-foreground">Weights in</h2>
		<div
			class="inline-flex overflow-hidden rounded-md border border-border"
			role="radiogroup"
			aria-label="Weight unit"
		>
			{#each [['metric', 'g, kg'], ['us', 'oz, lb']] as const as [w, label] (w)}
				<button
					type="button"
					role="radio"
					aria-checked={weight === w}
					class="{seg} {weight === w
						? 'bg-selected text-selected-foreground'
						: 'text-muted-foreground hover:bg-accent'}"
					onclick={() => (weight = w)}>{label}</button
				>
			{/each}
		</div>
		<p class="text-xs text-muted-foreground">For this browser; labels print both.</p>
	</section>

	<section class="space-y-2">
		<label for="welcome-zone" class="block text-sm font-semibold text-foreground">Time zone</label>
		<input
			id="welcome-zone"
			type="text"
			list="welcome-zones"
			bind:value={timeZone}
			placeholder="America/Chicago"
			spellcheck="false"
			autocomplete="off"
			class="{input} w-64"
		/>
		<datalist id="welcome-zones"
			>{#each zones as z (z)}<option value={z}></option>{/each}</datalist
		>
		<p class="text-xs text-muted-foreground">
			The day a roast falls on, for labels and reports. This browser's zone is filled in.
		</p>
	</section>

	<div class="flex flex-wrap items-center gap-3 border-t border-border pt-4">
		<button
			onclick={save}
			disabled={busy}
			class="rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
			>Save and add a roaster</button
		>
		<button
			onclick={notNow}
			disabled={busy}
			class="text-sm text-muted-foreground hover:text-foreground">Not now</button
		>
		{#if error}<span class="text-sm text-red-400">{error}</span>{/if}
	</div>
</div>
