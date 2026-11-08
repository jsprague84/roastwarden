<script lang="ts">
	/** Roast defaults stored on the server: they apply to every roaster and screen. */
	import { settings } from '#lib/api/settings.js';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { HELP } from '#lib/constants/help.js';
	import { setRorConfig } from '#lib/stores/telemetry.js';
	import { aucConfig } from '#lib/roast/stats.js';
	import {
		toDisplay,
		fromDisplay,
		roundForInput,
		tempUnit,
		weightUnit
	} from '#lib/stores/units.svelte.js';
	import WeightInput from '#lib/components/ui/WeightInput.svelte';
	import { isOn } from '#lib/stores/modules.svelte.js';

	let lookahead = $state(20);
	let lookaheadAdapt = $state(true);
	let lookaheadSaved = $state(false);
	// Auto DROP (docs/replay.md): the countdown before a drive marks DROP itself
	let dropCountdown = $state(10);
	let dropCountdownSaved = $state(false);
	let targetDtr = $state(20);
	let targetDtrSaved = $state(false);
	let rorWindow = $state(30);
	let rorAlgorithm = $state('moving_average');
	let rorSaved = $state(false);
	// AUC (docs/statistics.md): stored in °C, shown in the display unit.
	let aucBaseC = $state(0);
	let aucBase = $state<number | string | null>(0);
	let aucStart = $state('charge');
	let aucSaved = $state(false);
	// Costs (docs/green-coffee.md)
	let currency = $state('USD');
	let doseEspresso = $state<number | string | null>(18);
	let doseFilter = $state<number | string | null>(15);
	let bagG = $state<number | null>(250);
	let bagBad = $state(false);
	let costSaved = $state('');
	// The rest before coffee is ready: bag labels' "best from" (docs/labels.md) and roasted coffee (docs/roasted.md)
	const REST = [
		{ key: 'rest_days_espresso_light', label: 'Espresso, light', def: 10 },
		{ key: 'rest_days_espresso', label: 'Espresso, medium or darker', def: 7 },
		{ key: 'rest_days_filter_light', label: 'Filter, light', def: 7 },
		{ key: 'rest_days_filter', label: 'Filter, medium or darker', def: 4 }
	];
	let rest = $state<Record<string, number | string | null>>(
		Object.fromEntries(REST.map((r) => [r.key, r.def]))
	);
	// Roasted coffee (docs/roasted.md): at its best until, and when it runs low
	const BEST = [
		{ key: 'best_until_days_light', label: 'At its best for, light', def: 35 },
		{ key: 'best_until_days', label: 'At its best for, darker', def: 28 },
		// A ground bag, from the day it was packed (docs/roasted.md, "Packed bags")
		{ key: 'best_until_days_ground', label: 'Ground, at its best for', def: 14, max: 60 }
	];
	let best = $state<Record<string, number | string | null>>(
		Object.fromEntries(BEST.map((r) => [r.key, r.def]))
	);
	let roastedLow = $state<number | null>(200);
	let roastedLowBad = $state(false);
	let roasterName = $state('');
	let printerUrl = $state('');
	let labelSaved = $state('');
	// Reports (docs/reports.md): the paper and units a report uses unless asked otherwise
	let reportPaper = $state('letter');
	let reportUnits = $state('C');
	let reportSaved = $state('');
	// The owner's time zone (`time_zone`, the server's zone.rs): dates for an asker that sends no offset
	const browserZone = Intl.DateTimeFormat().resolvedOptions().timeZone || '';
	const zones: string[] =
		(Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf?.(
			'timeZone'
		) ?? [];
	let timeZone = $state('');
	let timeZoneStored = $state('');
	let timeZoneSaved = $state('');
	// Where the server's links point (`public_url`, approvals.rs link_base)
	let publicUrl = $state('');
	let publicUrlStored = $state('');
	let publicUrlSaved = $state('');

	$effect(() => {
		settings.get().then((s) => {
			const val = parseInt(s['profile_lookahead_seconds'] ?? '20', 10);
			if (!isNaN(val)) lookahead = val;
			lookaheadAdapt = (s['profile_lookahead_adapt'] ?? 'true') !== 'false';
			const cd = parseFloat(s['auto_drop_countdown_seconds'] ?? '10');
			if (Number.isFinite(cd)) dropCountdown = cd;
			const dtr = parseFloat(s['target_dtr_pct'] ?? '20');
			if (Number.isFinite(dtr)) targetDtr = dtr;
			const rorW = parseInt(s['ror_window_seconds'] ?? '30', 10);
			if (!isNaN(rorW)) rorWindow = rorW;
			rorAlgorithm = s['ror_smoothing_algorithm'] ?? 'moving_average';
			const a = aucConfig(s);
			aucBaseC = a.base;
			aucBase = a.base ? roundForInput(toDisplay(a.base), 0) : 0;
			aucStart = a.start;
			currency = s['currency'] || 'USD';
			doseEspresso = Number(s['dose_espresso_g'] || 18);
			doseFilter = Number(s['dose_filter_g'] || 15);
			bagG = Number(s['bag_g'] || 250);
			for (const r of REST) rest[r.key] = Number(s[r.key] || r.def);
			for (const r of BEST) best[r.key] = Number(s[r.key] || r.def);
			roastedLow = Number(s['roasted_low_g'] ?? 200);
			roasterName = s['label_roaster_name'] || '';
			printerUrl = s['label_printer_url'] || '';
			reportPaper = s['report_paper'] || 'letter';
			reportUnits = s['report_units'] || 'C';
			timeZoneStored = s['time_zone'] || '';
			timeZone = timeZoneStored || browserZone;
			publicUrlStored = s['public_url'] || '';
			publicUrl = publicUrlStored || location.origin;
		});
	});

	function flash(set: (v: boolean) => void) {
		set(true);
		setTimeout(() => set(false), 2000);
	}

	function saveTargetDtr() {
		targetDtr = Math.max(5, Math.min(40, Math.round(targetDtr * 10) / 10));
		settings
			.set('target_dtr_pct', String(targetDtr))
			.then(() => flash((v) => (targetDtrSaved = v)));
	}

	function saveLookahead() {
		lookahead = Math.max(5, Math.min(60, lookahead));
		Promise.all([
			settings.set('profile_lookahead_seconds', String(lookahead)),
			settings.set('profile_lookahead_adapt', String(lookaheadAdapt))
		]).then(() => flash((v) => (lookaheadSaved = v)));
	}

	function saveDropCountdown() {
		dropCountdown = Math.max(3, Math.min(60, Math.round(Number(dropCountdown) || 10)));
		settings
			.set('auto_drop_countdown_seconds', String(dropCountdown))
			.then(() => flash((v) => (dropCountdownSaved = v)));
	}

	function saveRorSettings() {
		rorWindow = Math.max(5, Math.min(120, rorWindow));
		Promise.all([
			settings.set('ror_window_seconds', String(rorWindow)),
			settings.set('ror_smoothing_algorithm', rorAlgorithm)
		]).then(() => {
			setRorConfig(rorWindow, rorAlgorithm);
			flash((v) => (rorSaved = v));
		});
	}

	function saveAuc() {
		const v = Number(aucBase);
		aucBaseC = Number.isFinite(v) && v !== 0 ? Math.round(fromDisplay(v) * 10) / 10 : 0;
		Promise.all([
			settings.set('auc_base_temp', String(aucBaseC)),
			settings.set('auc_start_event', aucStart)
		]).then(() => flash((v) => (aucSaved = v)));
	}

	function saveCosts() {
		if (bagBad || bagG == null) {
			costSaved = `The bag is not a weight: a number of ${weightUnit()}, or 250 g, 12 oz`;
			return;
		}
		settings
			.setMany({
				currency: currency.trim().toUpperCase(),
				dose_espresso_g: String(doseEspresso),
				dose_filter_g: String(doseFilter),
				bag_g: String(bagG)
			})
			.then(() => {
				costSaved = 'Saved.';
				setTimeout(() => (costSaved = ''), 2000);
			})
			.catch(
				(e) =>
					(costSaved = e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e))
			);
	}

	function saveLabels() {
		if (roastedLowBad) {
			labelSaved = `Running low is not a weight: a number of ${weightUnit()}, or 200 g, 8 oz`;
			return;
		}
		const values: Record<string, string> = {
			label_roaster_name: roasterName.trim(),
			label_printer_url: printerUrl.trim()
		};
		for (const r of REST) values[r.key] = String(Math.round(Number(rest[r.key] ?? r.def)));
		for (const r of BEST) values[r.key] = String(Math.round(Number(best[r.key] ?? r.def)));
		values['roasted_low_g'] = String(Number(roastedLow ?? 200));
		settings
			.setMany(values)
			.then(() => {
				labelSaved = 'Saved.';
				setTimeout(() => (labelSaved = ''), 2000);
			})
			.catch(
				(e) =>
					(labelSaved = e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e))
			);
	}

	function saveReports() {
		settings
			.setMany({ report_paper: reportPaper, report_units: reportUnits })
			.then(() => {
				reportSaved = 'Saved.';
				setTimeout(() => (reportSaved = ''), 2000);
			})
			.catch(
				(e) =>
					(reportSaved = e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e))
			);
	}

	function saveTimeZone() {
		const v = timeZone.trim();
		settings
			.set('time_zone', v)
			.then(() => {
				timeZoneStored = v;
				timeZoneSaved = 'Saved.';
				setTimeout(() => (timeZoneSaved = ''), 2000);
			})
			.catch(
				(e) =>
					(timeZoneSaved =
						e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e))
			);
	}

	function savePublicUrl() {
		const v = publicUrl.trim().replace(/\/+$/, '');
		settings
			.set('public_url', v)
			.then(() => {
				publicUrl = v;
				publicUrlStored = v;
				publicUrlSaved = 'Saved.';
				setTimeout(() => (publicUrlSaved = ''), 2000);
			})
			.catch(
				(e) =>
					(publicUrlSaved =
						e instanceof Error ? e.message.replace(/^API error \d+: /, '') : String(e))
			);
	}

	const input =
		'w-24 rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus';
	const save =
		'rounded-md bg-action px-4 py-2 text-sm font-medium text-action-foreground hover:bg-action-hover';
</script>

<p class="text-sm text-muted-foreground">
	Stored on the server; they apply to every roaster and every screen.
</p>

<div class="mt-4 divide-y divide-border border-y border-border">
	<section class="py-5">
		<label for="target-dtr" class="block text-sm font-semibold text-foreground"
			>Development target (%)</label
		>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			After first crack the scope shows when to drop for this development ratio (time from first
			crack ÷ total time). A profile's own first crack and total time take precedence. A profile
			with a roast level also gets its expected development time from roast guidance (Profiles → the
			profile → Roast level), judged in seconds.
		</p>
		<div class="mt-2 flex items-center gap-2">
			<input
				id="target-dtr"
				type="number"
				min="5"
				max="40"
				step="0.5"
				bind:value={targetDtr}
				onblur={saveTargetDtr}
				class={input}
			/>
			<button onclick={saveTargetDtr} class={save}>Save</button>
			{#if targetDtrSaved}<span class="text-sm text-green-400">Saved.</span>{/if}
		</div>
	</section>

	<section class="py-5">
		<label for="lookahead" class="block text-sm font-semibold text-foreground"
			>Profile lookahead (seconds)</label
		>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			When a profile drives the roaster in Setpoint mode, the setpoint is the profile BT this far
			ahead, so the PID starts turning before the curve does. Higher is smoother but leads more.
			Default 20 s.
		</p>
		<div class="mt-2 flex flex-wrap items-center gap-2">
			<input
				id="lookahead"
				type="number"
				min="5"
				max="60"
				step="1"
				bind:value={lookahead}
				onblur={saveLookahead}
				class={input}
			/>
			<label class="flex items-center gap-1.5 text-sm text-foreground">
				<input type="checkbox" bind:checked={lookaheadAdapt} onchange={saveLookahead} /> Adapt to
				the roaster
				<Hint inline help={HELP.lookaheadAdapt} />
			</label>
			<button onclick={saveLookahead} class={save}>Save</button>
			{#if lookaheadSaved}<span class="text-sm text-green-400">Saved.</span>{/if}
		</div>
	</section>

	<section class="py-5">
		<label for="drop-countdown" class="block text-sm font-semibold text-foreground"
			>Auto DROP countdown (seconds)</label
		>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			A profile with auto DROP on (Profiles → the profile) lets Profile drive mark DROP by itself at
			the profile's drop. The Roast page counts down this long first, with Cancel, and phones get a
			notice; the countdown ends on the drop itself. 3–60 s, default 10 s. It applies to drives
			started from now on.
		</p>
		<div class="mt-2 flex items-center gap-2">
			<input
				id="drop-countdown"
				type="number"
				min="3"
				max="60"
				step="1"
				bind:value={dropCountdown}
				onblur={saveDropCountdown}
				class={input}
			/>
			<button onclick={saveDropCountdown} class={save}>Save</button>
			{#if dropCountdownSaved}<span class="text-sm text-green-400">Saved.</span>{/if}
		</div>
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Area under the curve (AUC)</h2>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			BT summed over time in {tempUnit()}·min, a single number for how much heat the beans saw: it
			runs from the landmark below to DROP, counting only BT above the base. History shows it per
			phase, Compare side by side, and the Roast page's Phases tab live against the profile. 0 and
			CHARGE count everything; Artisan users often start at the turning point.
		</p>
		<div class="mt-2 flex flex-wrap items-end gap-3">
			<label class="text-sm text-foreground"
				>From
				<select
					bind:value={aucStart}
					class="mt-1 block rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus"
				>
					<option value="charge">CHARGE</option>
					<option value="turning_point">Turning point</option>
					<option value="drying_end">Dry end</option>
					<option value="first_crack_start">First crack</option>
				</select>
			</label>
			<label class="text-sm text-foreground"
				>Base ({tempUnit()})
				<input type="number" step="1" bind:value={aucBase} class="{input} mt-1 block" />
			</label>
			<button onclick={saveAuc} class={save}>Save</button>
			{#if aucSaved}<span class="text-sm text-green-400"
					>Saved. Roasts completed from now on store it this way.</span
				>{/if}
		</div>
	</section>

	{#if isOn('costs')}
		<section class="py-5">
			<h2 class="text-sm font-semibold text-foreground">Coffee costs</h2>
			<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
				Purchases are priced in this currency unless one says otherwise; a purchase in another
				currency is kept, but left out of the beans' cost per kg. A roast's page shows its coffee
				per kg roasted, per dose and per bag of these sizes.
			</p>
			<div class="mt-2 flex flex-wrap items-end gap-3">
				<label class="text-sm text-foreground"
					>Currency<input
						bind:value={currency}
						maxlength="3"
						class="{input} mt-1 block uppercase"
					/></label
				>
				<label class="text-sm text-foreground"
					>Espresso dose (g)<input
						type="number"
						min="1"
						max="100"
						step="0.5"
						bind:value={doseEspresso}
						class="{input} mt-1 block"
					/></label
				>
				<label class="text-sm text-foreground"
					>Filter dose (g)<input
						type="number"
						min="1"
						max="100"
						step="0.5"
						bind:value={doseFilter}
						class="{input} mt-1 block"
					/></label
				>
				<label class="text-sm text-foreground"
					>Bag ({weightUnit()})<WeightInput
						bind:grams={bagG}
						bind:invalid={bagBad}
						class="{input} mt-1 block"
					/></label
				>
				<button onclick={saveCosts} class={save}>Save</button>
				{#if costSaved}<span
						class="text-sm {costSaved === 'Saved.' ? 'text-green-400' : 'text-red-400'}"
						>{costSaved}</span
					>{/if}
			</div>
		</section>
	{/if}

	<section class="py-5">
		<div class="flex items-center gap-1.5">
			<label for="time-zone" class="text-sm font-semibold text-foreground">Time zone</label>
			<Hint inline help={HELP.timeZone} />
		</div>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			The day a roast falls on for labels, share pages, roasted coffee and reports made by the CLI
			or an agent. This dashboard always uses your browser's.
		</p>
		<div class="mt-2 flex flex-wrap items-center gap-2">
			<input
				id="time-zone"
				type="text"
				list="time-zones"
				bind:value={timeZone}
				placeholder="America/Chicago"
				spellcheck="false"
				autocomplete="off"
				class="{input} w-64"
			/>
			<datalist id="time-zones"
				>{#each zones as z (z)}<option value={z}></option>{/each}</datalist
			>
			<button onclick={saveTimeZone} class={save}>Save</button>
			{#if timeZoneSaved}<span
					class="text-sm {timeZoneSaved === 'Saved.' ? 'text-green-400' : 'text-red-400'}"
					>{timeZoneSaved}</span
				>
			{:else if !timeZoneStored}<span class="text-sm text-muted-foreground"
					>Not set, so UTC: your browser's zone is filled in, Save keeps it.</span
				>{/if}
		</div>
	</section>

	<section class="py-5">
		<div class="flex items-center gap-1.5">
			<label for="public-url" class="text-sm font-semibold text-foreground">Public address</label>
			<Hint inline help={HELP.publicUrl} />
		</div>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			Where share pages, label QR codes and approval links point: the address others open this
			server at.
		</p>
		<div class="mt-2 flex flex-wrap items-center gap-2">
			<input
				id="public-url"
				type="url"
				bind:value={publicUrl}
				placeholder="https://roast.example.com"
				spellcheck="false"
				autocomplete="off"
				class="{input} w-80"
			/>
			<button onclick={savePublicUrl} class={save}>Save</button>
			{#if publicUrlSaved}<span
					class="text-sm {publicUrlSaved === 'Saved.' ? 'text-green-400' : 'text-red-400'}"
					>{publicUrlSaved}</span
				>
			{:else if !publicUrlStored}<span class="text-sm text-muted-foreground"
					>Not set, so each link uses the address its request came to: this page's address is filled
					in, Save keeps it.</span
				>{/if}
		</div>
	</section>

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Rest, roasted coffee and bag labels</h2>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			Roasted coffee is ready from the roast date plus a rest: the Beans page's Roasted coffee and a
			label's “best from” both read it. Defaults from roasters' published guidance: light roasts and
			espresso rest longer (docs/labels.md). History → a roast → Labels… prints labels.
		</p>
		<div class="mt-3 flex flex-wrap items-end gap-4">
			{#each REST as r (r.key)}
				<label class="text-sm text-foreground"
					>{r.label} (days)<input
						type="number"
						min="0"
						max="90"
						step="1"
						bind:value={rest[r.key]}
						class="{input} mt-1 block"
					/></label
				>
			{/each}
		</div>
		<div class="mt-3 flex flex-wrap items-end gap-4">
			{#each BEST as r (r.key)}
				<label class="text-sm text-foreground"
					>{r.label} (days)<input
						type="number"
						min="1"
						max={r.max ?? 365}
						step="1"
						bind:value={best[r.key]}
						class="{input} mt-1 block"
					/></label
				>
			{/each}
			<label class="text-sm text-foreground"
				>Roasted coffee running low below ({weightUnit()})<WeightInput
					bind:grams={roastedLow}
					bind:invalid={roastedLowBad}
					class="{input} mt-1 block"
				/></label
			>
		</div>
		<div class="mt-3 flex flex-wrap items-end gap-4">
			<label class="text-sm text-foreground"
				>Roaster name on labels<input
					type="text"
					bind:value={roasterName}
					placeholder="optional"
					class="mt-1 block w-64 rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus"
				/></label
			>
			<label class="text-sm text-foreground"
				>Label printer (IPP)<input
					type="text"
					bind:value={printerUrl}
					placeholder="ipp://192.168.1.143/ipp/print"
					class="mt-1 block w-72 max-w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus"
				/></label
			>
			<button onclick={saveLabels} class={save}>Save</button>
			{#if labelSaved}<span
					class="text-sm {labelSaved === 'Saved.' ? 'text-green-400' : 'text-red-400'}"
					>{labelSaved}</span
				>{/if}
		</div>
	</section>

	{#if isOn('reports')}
		<section class="py-5">
			<h2 class="text-sm font-semibold text-foreground">Reports</h2>
			<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
				The paper and units a report uses unless the Reports page or an agent asks for others.
				Reports keep their own units, so a PDF reads the same wherever it is opened.
			</p>
			<div class="mt-3 flex flex-wrap items-end gap-4">
				<label class="text-sm text-foreground"
					>Paper
					<select
						bind:value={reportPaper}
						class="mt-1 block rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus"
					>
						<option value="letter">US Letter</option>
						<option value="a4">A4</option>
					</select>
				</label>
				<label class="text-sm text-foreground"
					>Temperatures
					<select
						bind:value={reportUnits}
						class="mt-1 block rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus"
					>
						<option value="C">°C</option>
						<option value="F">°F</option>
					</select>
				</label>
				<button onclick={saveReports} class={save}>Save</button>
				{#if reportSaved}<span
						class="text-sm {reportSaved === 'Saved.' ? 'text-green-400' : 'text-red-400'}"
						>{reportSaved}</span
					>{/if}
			</div>
		</section>
	{/if}

	<section class="py-5">
		<h2 class="text-sm font-semibold text-foreground">Rate of rise</h2>
		<p class="mt-1 max-w-2xl text-sm text-muted-foreground">
			The server computes one live RoR from BT for every screen and the alarms with these settings;
			a change applies at once.
		</p>
		<div class="mt-3 grid max-w-lg gap-4">
			<div>
				<label for="ror-window" class="block text-sm font-medium text-foreground"
					>Smoothing window ({rorWindow} s)</label
				>
				<input
					id="ror-window"
					type="range"
					min="5"
					max="120"
					step="5"
					bind:value={rorWindow}
					class="mt-2 w-full accent-action"
				/>
				<div class="mt-0.5 flex justify-between text-xs text-muted-foreground">
					<span>5 s (responsive)</span><span>120 s (smooth)</span>
				</div>
				<Hint class="mt-1" help={HELP.rorWindow} />
			</div>
			<div>
				<label for="ror-algorithm" class="block text-sm font-medium text-foreground"
					>Smoothing algorithm</label
				>
				<select
					id="ror-algorithm"
					bind:value={rorAlgorithm}
					class="mt-2 w-full rounded-md border border-border bg-input px-3 py-2 text-sm text-foreground focus:border-focus focus:outline-none focus:ring-1 focus:ring-focus"
				>
					<option value="moving_average">Moving average</option>
					<option value="weighted_moving_average">Weighted moving average</option>
					<option value="savitzky_golay">Savitzky-Golay</option>
				</select>
				<Hint
					class="mt-1"
					help={rorAlgorithm === 'weighted_moving_average'
						? HELP.rorWeighted
						: rorAlgorithm === 'savitzky_golay'
							? HELP.rorSavitzkyGolay
							: HELP.rorMovingAverage}
				/>
			</div>
			<div class="flex items-center gap-2">
				<button onclick={saveRorSettings} class={save}>Save</button>
				{#if rorSaved}<span class="text-sm text-green-400">Saved.</span>{/if}
			</div>
		</div>
	</section>
</div>
