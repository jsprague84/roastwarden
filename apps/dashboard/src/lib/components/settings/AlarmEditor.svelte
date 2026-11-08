<script lang="ts">
	/**
	 * Roast alarms and auto events (docs/alarms.md). Rules are evaluated on the
	 * server for every active roast; a fired rule can act on the roaster.
	 */
	import { resolve } from '$app/paths';
	import { Plus, Trash2, Download, Upload, Sparkles } from '@lucide/svelte';
	import { roasterSetup } from '#lib/api/roasters.js';
	import { settings } from '#lib/api/settings.js';
	import { listDevices } from '#lib/api/devices.js';
	import { notifications } from '#lib/stores/notifications.js';
	import {
		CONDITIONS,
		LANDMARKS,
		ACTIONS,
		newAction,
		normalizeRule,
		parseImport,
		describeActions,
		suggestedAlarms,
		unitOf,
		declaredChannels,
		channelRuleProblem,
		channelIsTemp,
		isChannelRule,
		isAheadRule,
		type DeclaredChannel,
		type AlarmRule,
		type AlarmAction,
		type UnitKind
	} from '#lib/alarms/alarms.js';
	import {
		toDisplay,
		fromDisplay,
		deltaToDisplay,
		deltaFromDisplay,
		tempUnit,
		rorUnit,
		roundForInput,
		unitState
	} from '#lib/stores/units.svelte.js';
	import { fromAlrm, toAlrm, isAlrm, type AlrmNote, type TempUnit } from '#lib/alarms/artisan.js';

	let alarms = $state<AlarmRule[]>([]);
	/** Every roaster's declared channels (docs/channels.md): what channel rules can watch. */
	let declared = $state<DeclaredChannel[]>([]);
	$effect(() => {
		listDevices()
			.then((devs) =>
				Promise.all(
					devs.map((d) =>
						roasterSetup
							.get(d.device_id)
							.then((s) => ({ device_id: s.device_id, channels: s.channels ?? [] }))
							.catch(() => ({ device_id: d.device_id, channels: [] }))
					)
				)
			)
			.then((setups) => (declared = declaredChannels(setups)))
			.catch(() => (declared = []));
	});
	let soundEnabled = $state(true);
	let autoEvents = $state(true);
	let autoDryC = $state(150);
	let autoDrop = $state(true);
	let loaded = $state(false);
	let saving = $state(false);
	let savedJson = $state('');
	let expanded = $state<number | null>(null);
	let importText = $state('');
	let showImport = $state(false);
	/** The unit Artisan ran in: an .alrm file does not say. */
	let alrmUnit = $state<TempUnit>(unitState.unit);
	/** What did not carry over in the last .alrm import or export. */
	let alrmReport = $state<{ title: string; notes: AlrmNote[] } | null>(null);
	const pastedIsAlrm = $derived.by(() => {
		try {
			return isAlrm(JSON.parse(importText));
		} catch {
			return false;
		}
	});

	$effect(() => {
		settings
			.get()
			.then((s) => {
				soundEnabled = s['alarm_sound_enabled'] !== 'false';
				autoEvents = s['auto_event_detection'] !== 'false';
				const d = parseFloat(s['auto_dry_temp'] ?? '150');
				autoDryC = Number.isFinite(d) ? d : 150;
				autoDrop = s['auto_drop'] !== 'false';
				try {
					const list = JSON.parse(s['roast_alarms'] ?? '[]');
					alarms = Array.isArray(list) ? list.map((r: AlarmRule) => normalizeRule(r)) : [];
				} catch {
					alarms = [];
				}
				savedJson = snapshot();
			})
			.finally(() => (loaded = true));
	});

	function snapshot() {
		return JSON.stringify([
			alarms.map(normalizeRule),
			soundEnabled,
			autoEvents,
			autoDryC,
			autoDrop
		]);
	}
	const dirty = $derived(loaded && snapshot() !== savedJson);

	// Thresholds and setpoint values are stored in °C; inputs show the display unit.
	const toShow = (u: UnitKind, v: number) =>
		u === 'temp'
			? roundForInput(toDisplay(v))
			: u === 'delta' || u === 'ror'
				? roundForInput(deltaToDisplay(v))
				: v;
	const fromShow = (u: UnitKind, v: number) =>
		u === 'temp' ? fromDisplay(v) : u === 'delta' || u === 'ror' ? deltaFromDisplay(v) : v;
	const unitLabel = (u: UnitKind) =>
		u === 'temp' || u === 'delta'
			? tempUnit()
			: u === 'ror'
				? rorUnit()
				: u === 'channel'
					? ''
					: 's';
	/** A rule's unit kind: a channel rule on a °C channel converts like a temperature. */
	const unitFor = (a: AlarmRule): UnitKind => {
		const u = unitOf(a.condition_type);
		if (u !== 'channel') return u;
		return channelIsTemp(a, declared) ? 'temp' : 'channel';
	};
	/** The unit after a channel rule's threshold: °C/°F, or the channel's own. */
	const channelUnit = (a: AlarmRule) => {
		const d = declared.find((c) => c.key === a.channel);
		return !d ? '' : d.unit === 'C' ? tempUnit() : d.unit;
	};

	function setThreshold(a: AlarmRule, raw: string) {
		const v = parseFloat(raw);
		if (!Number.isNaN(v)) a.threshold = fromShow(unitFor(a), v);
	}

	function add(rule?: AlarmRule) {
		alarms = [
			...alarms,
			rule ?? {
				name: 'New alarm',
				condition_type: 'temp_above',
				threshold: 230,
				enabled: true,
				actions: []
			}
		];
		expanded = alarms.length - 1;
	}

	function addSuggested() {
		const have = new Set(alarms.map((a) => a.name));
		const extra = suggestedAlarms().filter((a) => !have.has(a.name));
		alarms = [...alarms, ...extra];
		notifications.add(
			extra.length
				? `Added ${extra.length} suggested alarm(s); review and save`
				: 'Suggested alarms are already in the list',
			'success'
		);
	}

	function remove(i: number) {
		alarms = alarms.filter((_, j) => j !== i);
		expanded = null;
	}

	function addAction(a: AlarmRule, type: AlarmAction['type']) {
		a.actions = [...(a.actions ?? []), newAction(type)];
	}

	function removeAction(a: AlarmRule, k: number) {
		a.actions = (a.actions ?? []).filter((_, j) => j !== k);
	}

	async function save() {
		saving = true;
		try {
			const rules = alarms.map(normalizeRule);
			await Promise.all([
				settings.set('roast_alarms', JSON.stringify(rules)),
				settings.set('alarm_sound_enabled', String(soundEnabled)),
				settings.set('auto_event_detection', String(autoEvents)),
				settings.set('auto_dry_temp', String(Math.round(autoDryC * 10) / 10)),
				settings.set('auto_drop', String(autoDrop))
			]);
			alarms = rules;
			savedJson = snapshot();
			window.dispatchEvent(new CustomEvent('roastwarden:alarms-updated'));
			notifications.add('Alarms saved', 'success');
		} catch (e) {
			notifications.add(`Could not save alarms: ${e instanceof Error ? e.message : e}`, 'error');
		} finally {
			saving = false;
		}
	}

	function exportJson() {
		const blob = new Blob([JSON.stringify(alarms.map(normalizeRule), null, 2)], {
			type: 'application/json'
		});
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = 'roastwarden-alarms.json';
		a.click();
		URL.revokeObjectURL(url);
	}

	function exportAlrm() {
		const { file, notes } = toAlrm(alarms.map(normalizeRule), unitState.unit);
		const blob = new Blob([JSON.stringify(file) + '\n'], { type: 'application/json' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = 'roastwarden-alarms.alrm';
		a.click();
		URL.revokeObjectURL(url);
		alrmReport = {
			title: `Exported ${file.alarmflags.length} Artisan row(s) in °${unitState.unit}`,
			notes
		};
	}

	async function importFile(e: Event) {
		const f = (e.currentTarget as HTMLInputElement).files?.[0];
		if (f) importText = await f.text();
	}

	function doImport(replace: boolean) {
		try {
			const parsed = JSON.parse(importText);
			let rules: AlarmRule[];
			if (isAlrm(parsed)) {
				const r = fromAlrm(parsed, alrmUnit);
				// eslint-disable-next-line svelte/prefer-svelte-reactivity -- scratch inside one import, not state
				const taken = new Set(replace ? [] : alarms.map((a) => a.name));
				rules = r.rules.map((x) => {
					let name = x.name;
					for (let k = 2; taken.has(name); k++) name = `${x.name} (${k})`;
					taken.add(name);
					return { ...x, name };
				});
				alrmReport = {
					title: `Imported ${rules.length} of ${parsed.alarmflags?.length ?? 0} Artisan row(s)`,
					notes: r.notes
				};
			} else {
				rules = parseImport(importText);
			}
			alarms = replace ? rules : [...alarms, ...rules];
			showImport = false;
			importText = '';
			notifications.add(`Imported ${rules.length} alarm(s); review and save`, 'success');
		} catch (e) {
			notifications.add(`Import failed: ${e instanceof Error ? e.message : e}`, 'error');
		}
	}

	const field = 'rounded-md border border-border bg-input px-2 py-1 text-sm text-foreground';
	const btn =
		'rounded-md border border-border px-2.5 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-40';
	const condLabel = (c: string) => CONDITIONS.find((x) => x.value === c)?.label ?? c;
	const landmarkLabel = (c: string | null | undefined) =>
		LANDMARKS.find((l) => l.value === c)?.label ?? c ?? '';
</script>

<!-- Settings → Alarms: flat like the other Settings pages; the section menu names it -->
<div>
	<p class="text-sm text-muted-foreground">
		Checked on the server for every active roast, so they work with no dashboard open. An alarm can
		just notify, or act on the roaster: set setpoint, fan, heater or mode, cut the heater, mark a
		landmark, pause or stop the replay.
	</p>

	{#if !loaded}
		<p class="mt-3 text-xs text-muted-foreground">Loading…</p>
	{:else}
		<!-- Auto events -->
		<div class="mt-4 rounded-md border border-border/50 bg-muted/30 p-3 text-sm">
			<label class="flex items-center gap-2 text-foreground">
				<input type="checkbox" bind:checked={autoEvents} class="h-4 w-4" />
				Mark landmarks automatically
			</label>
			<div class="mt-2 grid gap-2 pl-6 sm:grid-cols-2 {autoEvents ? '' : 'opacity-50'}">
				<p class="text-muted-foreground sm:col-span-2">
					Turning point: always on (the BT minimum after charge; none on a cold charge, where BT
					never dips).
				</p>
				<label class="flex items-center gap-2 text-foreground">
					Dry end at BT
					<input
						type="number"
						step="1"
						value={roundForInput(toDisplay(autoDryC), 0)}
						oninput={(e) => {
							const v = parseFloat(e.currentTarget.value);
							if (!Number.isNaN(v)) autoDryC = fromDisplay(v);
						}}
						disabled={!autoEvents}
						class="{field} w-20"
					/>
					{tempUnit()}
				</label>
				<label
					class="flex items-center gap-2 text-foreground"
					title="BT falls 15 °C below its peak at −20 °C/min or faster for 5 s, once first crack is marked or BT passed 190 °C"
				>
					<input type="checkbox" bind:checked={autoDrop} disabled={!autoEvents} class="h-4 w-4" />
					Drop (sustained BT fall after development)
				</label>
				<p class="text-xs text-muted-foreground sm:col-span-2">
					First crack stays a prompt on the roast page: an RoR dip is too unreliable to mark on its
					own.
				</p>
			</div>
		</div>

		<!-- Delivery -->
		<div class="mt-3 flex flex-wrap items-center justify-between gap-3 text-sm">
			<label class="flex items-center gap-2 text-foreground">
				<input type="checkbox" bind:checked={soundEnabled} class="h-4 w-4" />
				Sound in the dashboard
			</label>
			<a
				href={resolve('/settings/notifications')}
				class="text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
			>
				Phones, Gotify, ntfy, Matrix and webhook: Notifications
			</a>
		</div>

		<!-- Rules -->
		<div class="mt-4 space-y-2">
			{#each alarms as a, i (i)}
				{@const u = unitFor(a)}
				{@const chProblem = channelRuleProblem(a, declared)}
				<div class="rounded-md border border-border/50 bg-muted/30 p-2.5">
					<div class="flex flex-wrap items-center gap-2 text-sm">
						<input type="checkbox" bind:checked={a.enabled} class="h-4 w-4" aria-label="Enabled" />
						<input
							type="text"
							bind:value={a.name}
							class="{field} min-w-0 flex-1"
							placeholder="Alarm name"
							aria-label="Name"
						/>
						<button class={btn} onclick={() => (expanded = expanded === i ? null : i)}
							>{expanded === i ? 'Done' : 'Edit'}</button
						>
						<button
							onclick={() => remove(i)}
							class="rounded p-1 text-muted-foreground hover:bg-red-500/15 hover:text-red-400"
							title="Remove alarm"
							aria-label="Remove alarm"
						>
							<Trash2 class="h-3.5 w-3.5" />
						</button>
					</div>
					{#if expanded !== i}
						<p class="mt-1 pl-6 text-xs text-muted-foreground">
							{condLabel(a.condition_type)}
							{#if isChannelRule(a)}{declared.find((c) => c.key === a.channel)?.label ??
									a.channel ??
									'?'}{/if}
							{a.condition_type === 'time_after_event'
								? `${a.threshold} s after ${landmarkLabel(a.reference_event)}`
								: `${toShow(u, a.threshold)} ${isChannelRule(a) ? channelUnit(a) : unitLabel(u)}`}
							{#if a.armed_after}· after {landmarkLabel(a.armed_after)}{/if}
							{#if a.requires}· if “{a.requires}” fired{/if}
							· {a.repeat ? `repeats (${a.cooldown_seconds ?? 60} s)` : 'once'}
							→ {describeActions(a.actions)}
						</p>
						{#if chProblem}<p class="mt-0.5 pl-6 text-xs text-highlight">{chProblem}</p>{/if}
					{:else}
						<div class="mt-2 space-y-2 pl-6 text-sm">
							<div class="flex flex-wrap items-center gap-2">
								<select bind:value={a.condition_type} class={field} aria-label="Condition">
									{#each CONDITIONS as c (c.value)}<option value={c.value} title={c.hint}
											>{c.label}</option
										>{/each}
								</select>
								{#if isChannelRule(a)}
									<select bind:value={a.channel} class={field} aria-label="Channel">
										<option value={null}>channel…</option>
										{#each declared as c (c.key)}<option
												value={c.key}
												title="Declared on {c.devices.join(', ')}">{c.label} ({c.key})</option
											>{/each}
										{#if a.channel && !declared.some((c) => c.key === a.channel)}<option
												value={a.channel}>{a.channel} (not declared)</option
											>{/if}
									</select>
								{/if}
								<input
									type="number"
									step="0.5"
									value={toShow(u, a.threshold)}
									oninput={(e) => setThreshold(a, e.currentTarget.value)}
									class="{field} w-20"
									aria-label="Threshold"
								/>
								<span class="text-muted-foreground"
									>{isChannelRule(a) ? channelUnit(a) : unitLabel(u)}</span
								>
								{#if a.condition_type === 'time_after_event'}
									<span class="text-muted-foreground">after</span>
									<select bind:value={a.reference_event} class={field} aria-label="Landmark">
										{#each LANDMARKS as l (l.value)}<option value={l.value}>{l.label}</option
											>{/each}
									</select>
								{/if}
							</div>
							{#if isAheadRule(a)}
								<p class="text-xs text-muted-foreground">
									From the live prediction’s RoR watch, from DRY end to DROP: what the roaster’s own
									model expects in the next two minutes (0 to 120 s ahead), or what the RoR shows
									now. The alarm’s message says why.
								</p>
							{/if}
							{#if a.condition_type.startsWith('delta_')}
								<p class="text-xs text-muted-foreground">
									BT minus the profile BT now (the session's profile, aligned like the replay). Use
									a negative number for "behind".
								</p>
							{/if}
							{#if isChannelRule(a)}
								<p class="text-xs {chProblem ? 'text-highlight' : 'text-muted-foreground'}">
									{chProblem ??
										`Fires on the roasters that declare ${a.channel}; on any other it never does.`}
								</p>
							{/if}
							<div class="flex flex-wrap items-center gap-2">
								<span class="text-muted-foreground">Armed after</span>
								<select bind:value={a.armed_after} class={field} aria-label="Armed after">
									<option value={null}>always</option>
									{#each LANDMARKS as l (l.value)}<option value={l.value}>{l.label}</option>{/each}
								</select>
								<span class="text-muted-foreground">and if</span>
								<select bind:value={a.requires} class={field} aria-label="Requires alarm">
									<option value={null}>—</option>
									{#each alarms.filter((x, j) => j !== i) as other (other.name)}<option
											value={other.name}>“{other.name}” fired</option
										>{/each}
								</select>
							</div>
							<div class="flex flex-wrap items-center gap-2">
								<label class="flex items-center gap-1.5"
									><input type="checkbox" bind:checked={a.repeat} class="h-4 w-4" /> Repeat when it comes
									back</label
								>
								{#if a.repeat}
									<span class="text-muted-foreground">at most every</span>
									<input
										type="number"
										min="0"
										step="5"
										bind:value={a.cooldown_seconds}
										placeholder="60"
										class="{field} w-20"
										aria-label="Cooldown"
									/>
									<span class="text-muted-foreground">s</span>
								{/if}
							</div>
							<div>
								<span class="text-muted-foreground">Then</span>
								<div class="mt-1 space-y-1">
									{#each a.actions ?? [] as act, k (k)}
										<div class="flex flex-wrap items-center gap-2">
											<span class="w-32 text-foreground"
												>{ACTIONS.find((x) => x.type === act.type)?.label}</span
											>
											{#if act.type === 'setpoint'}
												<input
													type="number"
													step="1"
													value={roundForInput(toDisplay(act.value), 0)}
													oninput={(e) => {
														const v = parseFloat(e.currentTarget.value);
														if (!Number.isNaN(v)) act.value = fromDisplay(v);
													}}
													class="{field} w-20"
													aria-label="Setpoint"
												/>
												<span class="text-muted-foreground">{tempUnit()}</span>
											{:else if act.type === 'fan' || act.type === 'heater'}
												<input
													type="number"
													min="0"
													max="100"
													step="1"
													bind:value={act.value}
													class="{field} w-20"
													aria-label="Percent"
												/>
												<span class="text-muted-foreground">%</span>
											{:else if act.type === 'mode'}
												<select bind:value={act.mode} class={field} aria-label="Mode"
													><option value="auto">auto (PID)</option><option value="manual"
														>manual</option
													></select
												>
											{:else if act.type === 'mark'}
												<select bind:value={act.event} class={field} aria-label="Landmark">
													{#each LANDMARKS as l (l.value)}<option value={l.value}>{l.label}</option
														>{/each}
												</select>
											{/if}
											<button
												onclick={() => removeAction(a, k)}
												class="rounded p-1 text-muted-foreground hover:text-red-400"
												aria-label="Remove action"><Trash2 class="h-3.5 w-3.5" /></button
											>
										</div>
									{/each}
									<select
										class="{field} text-muted-foreground"
										value=""
										onchange={(e) => {
											const t = e.currentTarget.value as AlarmAction['type'];
											if (t) addAction(a, t);
											e.currentTarget.value = '';
										}}
										aria-label="Add action"
									>
										<option value="">+ add action (otherwise notify only)</option>
										{#each ACTIONS as x (x.type)}<option value={x.type}>{x.label}</option>{/each}
									</select>
								</div>
							</div>
						</div>
					{/if}
				</div>
			{:else}
				<p class="text-sm text-muted-foreground">No alarms yet.</p>
			{/each}
		</div>

		{#if showImport}
			<div class="mt-3 space-y-2 rounded-md border border-border/50 p-3">
				<input
					type="file"
					accept="application/json,.json,.alrm"
					onchange={importFile}
					class="text-xs text-muted-foreground"
				/>
				<textarea
					bind:value={importText}
					rows="5"
					placeholder="…or paste the JSON here (a Roastwarden export or an Artisan .alrm)"
					class="{field} w-full font-mono text-xs"></textarea>
				{#if pastedIsAlrm}
					<div class="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
						Artisan alarm file. Its temperatures are in
						{#each ['C', 'F'] as const as u (u)}
							<label class="flex items-center gap-1 text-foreground"
								><input
									type="radio"
									name="alrm-unit"
									checked={alrmUnit === u}
									onchange={() => (alrmUnit = u)}
								/>°{u}</label
							>
						{/each}
						(the file does not say; pick the unit Artisan ran in).
					</div>
				{/if}
				<div class="flex gap-2">
					<button class={btn} disabled={!importText.trim()} onclick={() => doImport(false)}
						>Add to list</button
					>
					<button class={btn} disabled={!importText.trim()} onclick={() => doImport(true)}
						>Replace list</button
					>
					<button class="{btn} border-transparent" onclick={() => (showImport = false)}
						>Cancel</button
					>
				</div>
			</div>
		{/if}

		{#if alrmReport}
			<div class="mt-3 rounded-md border border-border p-3 text-xs" role="status">
				<div class="flex items-center gap-2">
					<span class="font-medium text-foreground">{alrmReport.title}</span>
					<span class="text-muted-foreground"
						>{alrmReport.notes.length
							? `${alrmReport.notes.length} note(s)`
							: 'everything carried over'}</span
					>
					<button class="{btn} ml-auto border-transparent" onclick={() => (alrmReport = null)}
						>Dismiss</button
					>
				</div>
				{#if alrmReport.notes.length}
					<ul class="mt-2 space-y-0.5">
						{#each alrmReport.notes as n, i (i)}
							<li>
								<span class="text-muted-foreground">{n.at}</span> ·
								<span class={n.kind === 'skipped' ? 'text-red-400' : 'text-amber-400'}
									>{n.kind === 'skipped' ? 'not carried over' : 'partly'}</span
								>: {n.text}
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		{/if}

		<div class="mt-3 flex flex-wrap items-center gap-2">
			<button class={btn} onclick={() => add()}
				><Plus class="mr-1 inline h-3.5 w-3.5" />Add alarm</button
			>
			<button
				class={btn}
				onclick={addSuggested}
				title="A temperature ceiling, a lost-telemetry stop, a cool-down after DROP, a behind-profile warning, and RoR crash, flick and stall warnings"
				><Sparkles class="mr-1 inline h-3.5 w-3.5" />Add suggested</button
			>
			<button class={btn} onclick={exportJson} disabled={!alarms.length}
				><Download class="mr-1 inline h-3.5 w-3.5" />Export</button
			>
			<button
				class={btn}
				onclick={exportAlrm}
				disabled={!alarms.length}
				title="An Artisan alarm file, temperatures in the unit shown here"
				><Download class="mr-1 inline h-3.5 w-3.5" />Export .alrm</button
			>
			<button class={btn} onclick={() => (showImport = !showImport)}
				><Upload class="mr-1 inline h-3.5 w-3.5" />Import</button
			>
			<button
				onclick={save}
				disabled={!dirty || saving}
				class="ml-auto rounded-md bg-action px-4 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-40"
			>
				{saving ? 'Saving…' : 'Save alarms'}
			</button>
		</div>
	{/if}
</div>
