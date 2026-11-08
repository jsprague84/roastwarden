<script lang="ts">
	/**
	 * A connection's mapping (docs/ingest-mapping.md, docs/add-roaster.md "Dashboard"): the
	 * device's own field names, unit, TC4 columns, MQTT topics and command templates, with the
	 * newest raw message and what the mapping makes of it, refreshed every 2 s while open. It
	 * saves through the connection's PUT, which checks the mapping (a 400 says what is wrong).
	 */
	import { untrack } from 'svelte';
	import { Plus, Trash2, Loader2, Check, ArrowUp } from '@lucide/svelte';
	import Hint from '#lib/components/ui/Hint.svelte';
	import { HELP } from '#lib/constants/help.js';
	import { roasterCheck, type IngestPreview, type RawMessage } from '#lib/api/adding.js';
	import type { DeviceConnection } from '#lib/api/devices.js';
	import {
		COMMAND_KEYS,
		READ_ROLES,
		READ_ROLE_LABEL,
		configWithMapping,
		emptyField,
		emptyTemplate,
		ingestOf,
		isRole,
		mappingFromConfig,
		mappingProblems,
		prettyPayload,
		type MappingForm,
		type MappingProtocol
	} from '#lib/roaster/mapping.js';
	import { PHIDGET_INPUTS } from '#lib/roaster/phidget.js';
	import { fmtTemp, tempUnit } from '#lib/stores/units.svelte.js';

	let {
		registryId,
		deviceId,
		conn,
		onsaved
	}: { registryId: string; deviceId: string; conn: DeviceConnection; onsaved?: () => void } =
		$props();

	const protocol = $derived(conn.protocol as MappingProtocol);
	const mqtt = $derived(protocol === 'mqtt');
	const tc4 = $derived(protocol === 'serial_tc4');
	/** A Phidgets board: its inputs as paths, °C, no commands */
	const phidget = $derived(protocol === 'phidget22');

	let form = $state<MappingForm>(mappingFromConfig('mqtt', {}));
	let saving = $state(false);
	let saveError = $state('');
	let saved = $state(false);

	// The saved config is the starting point; a save (or another connection) starts again.
	$effect(() => {
		form = mappingFromConfig(conn.protocol as MappingProtocol, conn.config);
		saveError = '';
	});

	const problems = $derived(mappingProblems(protocol, form));

	// --- The newest message and the preview, every 2 s ---
	let raw = $state<RawMessage[]>([]);
	let preview = $state<IngestPreview | null>(null);
	let previewError = $state('');

	async function poll() {
		const ingest = ingestOf(protocol, $state.snapshot(form) as MappingForm);
		const [r, p] = await Promise.allSettled([
			roasterCheck.raw(deviceId),
			roasterCheck.preview(deviceId, conn.id, ingest)
		]);
		if (r.status === 'fulfilled')
			raw = r.value.filter((m) => m.connection_id === conn.id || m.connection_id == null);
		if (p.status === 'fulfilled') {
			preview = p.value;
			previewError = '';
		} else {
			preview = null;
			const e = p.reason;
			previewError = (e instanceof Error ? e.message : String(e)).replace(/^API error \d+: /, '');
		}
	}

	$effect(() => {
		// Again when the roaster or connection changes; the form's edits wait for the next tick.
		void deviceId;
		void conn.id;
		untrack(() => void poll());
		const t = setInterval(() => void poll(), 2000);
		return () => clearInterval(t);
	});

	const messages = $derived(preview?.messages?.length ? preview.messages : raw);

	async function save() {
		saving = true;
		saveError = '';
		saved = false;
		try {
			await roasterCheck.saveConfig(
				registryId,
				conn.id,
				configWithMapping(protocol, conn.config, $state.snapshot(form) as MappingForm)
			);
			saved = true;
			onsaved?.();
		} catch (e) {
			saveError = (e instanceof Error ? e.message : String(e)).replace(/^API error \d+: /, '');
		} finally {
			saving = false;
		}
	}

	function revert() {
		form = mappingFromConfig(protocol, conn.config);
		saveError = '';
	}

	/** A new row: bt first; a Phidget's on the first input not read yet. */
	function newField() {
		const row = emptyField(form.fields.length === 0 ? 'bt' : '');
		if (phidget)
			row.path = PHIDGET_INPUTS.find((c) => !form.fields.some((f) => f.path.trim() === c)) ?? 'ch0';
		return row;
	}

	function moveUp(i: number) {
		if (i <= 0) return;
		const c = [...form.columns];
		[c[i - 1], c[i]] = [c[i], c[i - 1]];
		form.columns = c;
	}

	const TEMP_ROLES = ['bt', 'et', 'ambient', 'setpoint'];
	function showValue(key: string, v: number | null): string {
		if (v == null) return '—';
		if (TEMP_ROLES.includes(key)) return `${fmtTemp(v, 1)}${tempUnit()}`;
		return `${Math.round(v * 1000) / 1000}`;
	}

	const input =
		'w-full rounded-md border border-border bg-input px-2 py-1 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-focus';
	const small =
		'inline-flex items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-foreground hover:bg-accent';
	const listId = $derived(`roles-${conn.id}`);
</script>

<datalist id={listId}>
	{#each READ_ROLES as r (r)}<option value={r}>{READ_ROLE_LABEL[r]}</option>{/each}
</datalist>

<div class="mt-3 space-y-4 border-t border-border pt-3" data-testid="mapping-editor">
	<div class="flex flex-wrap items-center justify-between gap-2">
		<div class="flex items-center gap-2">
			<h3 class="text-sm font-semibold text-foreground">Mapping</h3>
			<Hint help={HELP.mapping} inline />
		</div>
		{#if phidget}
			<span class="text-xs text-muted-foreground">The board sends °C</span>
		{:else}
			<div class="flex items-center gap-3 text-xs">
				<span class="text-muted-foreground">The device sends</span>
				{#each ['C', 'F'] as const as u (u)}
					<button
						type="button"
						onclick={() => (form.units = u)}
						class="border-b-2 pb-0.5 {form.units === u
							? 'border-action text-foreground'
							: 'border-transparent text-muted-foreground hover:text-foreground'}"
					>
						°{u}
					</button>
				{/each}
			</div>
		{/if}
	</div>

	{#if mqtt}
		<div class="grid gap-2 sm:grid-cols-2">
			<label class="text-xs text-muted-foreground">
				Telemetry topic
				<input
					class="{input} mt-1"
					bind:value={form.telemetryTopic}
					placeholder={`${form.topicPrefix || `roaster/${deviceId}`}/telemetry`}
				/>
			</label>
			<label class="text-xs text-muted-foreground">
				Command prefix (commands go to …/control/&lt;command&gt;)
				<input
					class="{input} mt-1"
					bind:value={form.topicPrefix}
					placeholder={`roaster/${deviceId}`}
				/>
			</label>
		</div>
	{/if}

	{#if tc4}
		<div>
			<p class="text-xs text-muted-foreground">
				The reply's columns by position: a role (bt, et, ambient, heater, fan…) or a name of your
				own; empty skips a column. A name declared as a °C channel follows the unit above;
				exhaust:F, :C or :none sets a column's own. No columns: the TC4 layout from the connection's
				settings.
			</p>
			<ol class="mt-2 space-y-1">
				{#each form.columns as _, i (i)}
					<li class="flex items-center gap-2">
						<span class="w-16 shrink-0 text-xs text-muted-foreground">column {i + 1}</span>
						<input class={input} list={listId} bind:value={form.columns[i]} placeholder="skip" />
						<button
							type="button"
							class={small}
							disabled={i === 0}
							onclick={() => moveUp(i)}
							aria-label="Move up"><ArrowUp class="h-3 w-3" /></button
						>
						<button
							type="button"
							class="{small} text-destructive"
							onclick={() => (form.columns = form.columns.filter((_, j) => j !== i))}
							aria-label="Remove column"><Trash2 class="h-3 w-3" /></button
						>
					</li>
				{/each}
			</ol>
			<button
				type="button"
				class="{small} mt-2"
				onclick={() => (form.columns = [...form.columns, ''])}
				><Plus class="h-3 w-3" /> Column</button
			>
		</div>
	{:else}
		<div class="overflow-x-auto">
			{#if phidget}
				<p class="mb-1 text-xs text-muted-foreground">
					Each row reads one of the board's inputs: a role (bt, et, ambient) or a name of your own,
					an extra channel to declare in the roaster's setup.
				</p>
			{/if}
			<table class="w-full text-left text-xs">
				<thead class="text-muted-foreground">
					<tr>
						<th class="py-1 pr-2 font-medium">Role or name</th>
						<th class="py-1 pr-2 font-medium">{phidget ? 'Input' : 'Path'}</th>
						<th class="py-1 pr-2 font-medium">Scale</th>
						<th class="py-1 pr-2 font-medium">Offset</th>
						{#if !phidget}<th class="py-1 pr-2 font-medium">Unit</th>{/if}
						{#if mqtt}<th class="py-1 pr-2 font-medium">Own topic</th>{/if}
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each form.fields as f, i (i)}
						<tr>
							<td class="py-0.5 pr-2"
								><input class={input} list={listId} bind:value={f.key} placeholder="bt" /></td
							>
							<td class="py-0.5 pr-2">
								{#if phidget}
									<select class={input} bind:value={f.path}>
										{#each PHIDGET_INPUTS as c (c)}<option value={c}>{c}</option>{/each}
									</select>
								{:else}
									<input class={input} bind:value={f.path} placeholder="data.BT" />
								{/if}
							</td>
							<td class="py-0.5 pr-2 w-16"
								><input
									class={input}
									bind:value={f.scale}
									placeholder="1"
									inputmode="decimal"
								/></td
							>
							<td class="py-0.5 pr-2 w-16"
								><input
									class={input}
									bind:value={f.offset}
									placeholder="0"
									inputmode="decimal"
								/></td
							>
							{#if !phidget}
								<td class="py-0.5 pr-2 w-16">
									<select
										class={input}
										bind:value={f.unit}
										disabled={isRole(f.key.trim())}
										title={isRole(f.key.trim())
											? 'A role follows the mapping’s units'
											: 'An extra channel’s own unit'}
									>
										<option value="">—</option>
										<option value="C">°C</option>
										<option value="F">°F</option>
									</select>
								</td>
							{/if}
							{#if mqtt}<td class="py-0.5 pr-2"
									><input
										class={input}
										bind:value={f.topic}
										placeholder="the telemetry topic"
									/></td
								>{/if}
							<td class="py-0.5">
								<button
									type="button"
									class="{small} text-destructive"
									onclick={() => (form.fields = form.fields.filter((_, j) => j !== i))}
									aria-label="Remove field"><Trash2 class="h-3 w-3" /></button
								>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
			<button
				type="button"
				class="{small} mt-2"
				onclick={() => (form.fields = [...form.fields, newField()])}
				><Plus class="h-3 w-3" /> Field</button
			>
			{#if form.fields.length === 0}
				<span class="ml-2 text-xs text-muted-foreground"
					>{phidget
						? 'No fields: BT ch0, ET ch1.'
						: "No fields: the device sends Roastwarden's own JSON."}</span
				>
			{/if}
		</div>
	{/if}

	{#if !phidget}
		<div>
			<h4 class="text-xs font-semibold text-foreground">Command templates</h4>
			<p class="text-xs text-muted-foreground">
				How a command is written; whether it is sent is the roaster's setup. <code>{'{value}'}</code
				> is the value.
			</p>
			{#if form.templates.length > 0}
				<div class="mt-1 overflow-x-auto">
					<table class="w-full text-left text-xs">
						<thead class="text-muted-foreground">
							<tr>
								<th class="py-1 pr-2 font-medium">Command</th>
								{#if mqtt}<th class="py-1 pr-2 font-medium">Topic</th>{/if}
								<th class="py-1 pr-2 font-medium">Payload</th>
								<th class="py-1 pr-2 font-medium">Scale</th>
								<th class="py-1 pr-2 font-medium">Decimals</th>
								<th></th>
							</tr>
						</thead>
						<tbody>
							{#each form.templates as t, i (i)}
								<tr>
									<td class="py-0.5 pr-2 w-32">
										<select class={input} bind:value={t.key}>
											{#each COMMAND_KEYS as k (k)}<option value={k}>{k}</option>{/each}
										</select>
									</td>
									{#if mqtt}<td class="py-0.5 pr-2"
											><input
												class={input}
												bind:value={t.topic}
												placeholder="<prefix>/control/<command>"
											/></td
										>{/if}
									<td class="py-0.5 pr-2"
										><input class={input} bind:value={t.payload} placeholder={'{value}'} /></td
									>
									<td class="py-0.5 pr-2 w-16"
										><input
											class={input}
											bind:value={t.scale}
											placeholder="1"
											inputmode="decimal"
										/></td
									>
									<td class="py-0.5 pr-2 w-16"
										><input
											class={input}
											bind:value={t.decimals}
											placeholder="1"
											inputmode="numeric"
										/></td
									>
									<td class="py-0.5">
										<button
											type="button"
											class="{small} text-destructive"
											onclick={() => (form.templates = form.templates.filter((_, j) => j !== i))}
											aria-label="Remove template"><Trash2 class="h-3 w-3" /></button
										>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
			<button
				type="button"
				class="{small} mt-2"
				onclick={() => {
					const next =
						COMMAND_KEYS.find((k) => !form.templates.some((t) => t.key === k)) ?? 'heater';
					form.templates = [...form.templates, emptyTemplate(next)];
				}}
			>
				<Plus class="h-3 w-3" /> Template
			</button>
		</div>
	{/if}

	{#if problems.length > 0}
		<ul class="space-y-0.5 text-xs text-highlight">
			{#each problems as p (p)}<li>{p}</li>{/each}
		</ul>
	{/if}

	<div class="flex flex-wrap items-center gap-2">
		<button
			type="button"
			onclick={save}
			disabled={saving || problems.length > 0}
			class="inline-flex items-center gap-1.5 rounded-md bg-action px-3 py-1.5 text-sm font-medium text-action-foreground hover:bg-action-hover disabled:opacity-50"
		>
			{#if saving}<Loader2 class="h-3.5 w-3.5 animate-spin" />{:else if saved}<Check
					class="h-3.5 w-3.5"
				/>{/if}
			Save mapping
		</button>
		<button
			type="button"
			onclick={revert}
			class="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-accent"
			>Revert</button
		>
		{#if saveError}<span class="text-sm text-destructive">{saveError}</span>{/if}
	</div>

	<div class="space-y-2">
		<h4 class="text-xs font-semibold text-foreground">Newest message</h4>
		{#if messages.length === 0}
			<p class="text-xs text-muted-foreground">
				Nothing received yet on this connection{mqtt ? ' (on the topics it claims)' : ''}. It
				refreshes every 2 s.
			</p>
		{:else}
			{#each messages as m, i (i)}
				<div>
					<p class="text-[11px] text-muted-foreground">
						{m.topic ?? m.protocol} · {new Date(m.received_at).toLocaleTimeString()}{m.truncated
							? ' · cut at 4 KB'
							: ''}
					</p>
					<pre
						class="scroll-thin mt-0.5 max-h-40 overflow-auto rounded-md border border-border bg-muted/30 p-2 text-[11px] text-foreground">{prettyPayload(
							m.payload
						)}</pre>
				</div>
			{/each}
		{/if}

		<h4 class="text-xs font-semibold text-foreground">What the mapping makes of it</h4>
		{#if previewError}
			<p class="text-xs text-destructive">{previewError}</p>
		{:else if preview}
			{#if preview.fields.length > 0}
				<table class="w-full text-left text-xs">
					<thead class="text-muted-foreground">
						<tr>
							<th class="py-1 pr-2 font-medium">Field</th>
							<th class="py-1 pr-2 font-medium">From</th>
							<th class="py-1 pr-2 font-medium">Read</th>
							<th class="py-1 pr-2 font-medium">Becomes</th>
						</tr>
					</thead>
					<tbody>
						{#each preview.fields as f, i (i)}
							<tr>
								<td class="py-0.5 pr-2 text-foreground"
									>{f.key}{f.field !== f.key ? ` → ${f.field}` : ''}</td
								>
								<td class="py-0.5 pr-2 font-mono text-muted-foreground">{f.source}</td>
								<td class="py-0.5 pr-2 font-mono text-muted-foreground"
									>{f.raw == null ? '—' : JSON.stringify(f.raw)}</td
								>
								<td class="py-0.5 pr-2 tabular-nums {f.problem ? 'text-highlight' : 'text-go'}"
									>{f.problem ?? showValue(f.key, f.value)}</td
								>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
			{#if preview.sample}
				<p class="text-xs text-muted-foreground">
					Sample: <code class="text-foreground">{JSON.stringify(preview.sample)}</code>
				</p>
			{:else if preview.messages.length > 0}
				<p class="text-xs text-highlight">
					No sample: the mapping gives neither BT nor ET from this message.
				</p>
			{/if}
			{#if preview.note}<p class="text-xs text-muted-foreground">{preview.note}</p>{/if}
		{:else}
			<p class="text-xs text-muted-foreground">…</p>
		{/if}
	</div>
</div>
